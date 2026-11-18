use std::fs::{self, File};
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use rand::Rng;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tiny_http::{Method, Response, Server, StatusCode};
use uuid::Uuid;

use crate::commands::fs_scope::Access;
use crate::commands::{ApiError, ApiResult, AppState};
use crate::core::pms_archive;

const SERVICE_TYPE: &str = "_pms._tcp.local.";
/// Crockford-style alphabet: no `I`, `L`, `O`, `U`, `0`, `1`, so a code read from
/// a screen cannot be mistyped that way. 32 symbols.
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTVWXYZ23456789";
const CODE_LEN: usize = 8;
/// 32^8 = 2^40: at one guess per second a brute force would need ~34 000 years,
/// and the receiver stops after `MAX_CODE_ATTEMPTS` anyway.
const MAX_CODE_ATTEMPTS: u32 = 3;
const RECEIVE_TTL: Duration = Duration::from_secs(120);

fn generate_code() -> String {
    let mut rng = rand::rngs::OsRng;
    (0..CODE_LEN)
        .map(|_| CODE_ALPHABET[rng.gen_range(0..CODE_ALPHABET.len())] as char)
        .collect()
}

/// Accepts what a person types: case-insensitive, spaces and dashes ignored.
fn normalize_code(value: &str) -> Option<String> {
    let mut out = String::with_capacity(CODE_LEN);
    for character in value.chars() {
        if character == '-' || character.is_whitespace() {
            continue;
        }
        let upper = character.to_ascii_uppercase();
        if CODE_ALPHABET.contains(&(upper as u8)) {
            out.push(upper);
        } else {
            return None;
        }
    }
    if out.len() == CODE_LEN {
        Some(out)
    } else {
        None
    }
}

/// Constant-time comparison: the code is a shared secret, so a byte-wise
/// comparison that stops at the first difference leaks timing.
fn code_matches(expected: &str, received: &str) -> bool {
    match normalize_code(received) {
        Some(received) => {
            let mut diff = 0u8;
            for (a, b) in expected.bytes().zip(received.bytes()) {
                diff |= a ^ b;
            }
            diff == 0
        }
        None => false,
    }
}

/// Proves that the sender knows the code without putting it in a header: the MAC
/// covers a digest of the body, so an interceptor cannot replay a captured
/// upload to another receiver either.
fn body_mac(code: &str, body_sha256: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = match Hmac::<Sha256>::new_from_slice(code.as_bytes()) {
        Ok(mac) => mac,
        Err(_) => return String::new(),
    };
    mac.update(b"PMS-TRANSFER-V1");
    mac.update(body_sha256);
    hex::encode(mac.finalize().into_bytes())
}

fn sha256_file(path: &Path) -> ApiResult<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = File::open(path).map_err(|error| ApiError::new("transfer_network", error.to_string()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| ApiError::new("transfer_network", error.to_string()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}
const DISCOVERY_PORT: u16 = 45873;
const DISCOVERY_REQUEST: &[u8] = b"PMS_DISCOVER_V1";
const MAX_ARCHIVE: u64 = 2 * 1024 * 1024 * 1024;

fn secure_file(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    let _ = path;
}

fn secure_dir(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
    }
    #[cfg(not(unix))]
    let _ = path;
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferPeer {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiveStarted {
    pub code: String,
    pub device_name: String,
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Serialize, Deserialize)]
struct DiscoveryReply {
    v: u8,
    name: String,
    port: u16,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReceivedArchive {
    pub archive_path: String,
    pub sender: String,
    pub destination: String,
    pub replace_current: bool,
}

struct ReceiverSession {
    stop: Arc<AtomicBool>,
    server: Arc<Server>,
    mdns: Option<ServiceDaemon>,
    discovery: Option<Arc<UdpSocket>>,
}

#[derive(Default)]
pub struct TransferManager {
    receiver: Mutex<Option<ReceiverSession>>,
}

impl TransferManager {
    fn stop(&self) {
        if let Ok(mut guard) = self.receiver.lock() {
            if let Some(session) = guard.take() {
                session.stop.store(true, Ordering::Relaxed);
                session.server.unblock();
                if let Some(mdns) = session.mdns {
                    let _ = mdns.shutdown();
                }
                drop(session.discovery);
            }
        }
    }
}

fn transfer_temp_dir(app: &AppHandle) -> ApiResult<PathBuf> {
    let base = app
        .path()
        .app_cache_dir()
        .map_err(|error| ApiError::new("transfer_storage", error.to_string()))?;
    let dir = base.join("transfer");
    fs::create_dir_all(&dir)
        .map_err(|error| ApiError::new("transfer_storage", error.to_string()))?;
    Ok(dir)
}

fn device_name() -> String {
    hostname::get()
        .ok()
        .and_then(|value| value.into_string().ok())
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .trim_end_matches('.')
                .trim_end_matches(".local")
                .to_string()
        })
        .unwrap_or_else(|| "PMS".to_string())
}

fn local_address() -> Result<IpAddr, ApiError> {
    let detected = local_ip_address::local_ip()
        .map_err(|error| ApiError::new("transfer_network", error.to_string()))?;
    if detected.is_ipv4() {
        return Ok(detected);
    }
    if let Ok(interfaces) = if_addrs::get_if_addrs() {
        for interface in interfaces {
            if let if_addrs::IfAddr::V4(address) = interface.addr {
                if !address.ip.is_loopback() && !address.ip.is_link_local() {
                    return Ok(IpAddr::V4(address.ip));
                }
            }
        }
    }
    Ok(detected)
}

fn dns_hostname() -> String {
    let host = device_name();
    let host = host.trim_end_matches('.').trim_end_matches(".local");
    format!("{}.local.", host.replace(' ', "-"))
}

fn header_safe(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_graphic() || character == ' ' {
                character
            } else {
                '?'
            }
        })
        .collect()
}

fn service_instance(name: &str) -> String {
    let mut value = name.to_string();
    while value.len() > 63 {
        value.pop();
    }
    value
}

fn register_service(port: u16, name: &str) -> Result<ServiceDaemon, ApiError> {
    let mdns =
        ServiceDaemon::new().map_err(|error| ApiError::new("transfer_mdns", error.to_string()))?;
    let props = [("name", name.to_string()), ("version", "1".to_string())];
    let service = ServiceInfo::new(
        SERVICE_TYPE,
        &service_instance(name),
        &dns_hostname(),
        local_address()?,
        port,
        &props[..],
    )
    .map_err(|error| ApiError::new("transfer_mdns", error.to_string()))?;
    mdns.register(service)
        .map_err(|error| ApiError::new("transfer_mdns", error.to_string()))?;
    Ok(mdns)
}

fn start_discovery_responder(
    port: u16,
    name: &str,
    stop: Arc<AtomicBool>,
) -> ApiResult<Arc<UdpSocket>> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, DISCOVERY_PORT))
        .map_err(|error| ApiError::new("transfer_discovery", error.to_string()))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(500)))
        .map_err(|error| ApiError::new("transfer_discovery", error.to_string()))?;
    let thread_socket = socket
        .try_clone()
        .map_err(|error| ApiError::new("transfer_discovery", error.to_string()))?;
    let reply_name = name.to_string();
    std::thread::spawn(move || {
        let mut buffer = [0u8; 256];
        while !stop.load(Ordering::Relaxed) {
            match thread_socket.recv_from(&mut buffer) {
                Ok((size, sender)) if &buffer[..size] == DISCOVERY_REQUEST => {
                    let reply = DiscoveryReply {
                        v: 1,
                        name: reply_name.clone(),
                        port,
                    };
                    if let Ok(payload) = serde_json::to_vec(&reply) {
                        let _ = thread_socket.send_to(&payload, sender);
                    }
                }
                Ok(_) | Err(_) => {}
            }
        }
    });
    Ok(Arc::new(socket))
}

fn discovery_targets() -> Vec<SocketAddr> {
    let mut targets = vec![SocketAddr::new(
        IpAddr::V4(Ipv4Addr::BROADCAST),
        DISCOVERY_PORT,
    )];
    if let Ok(interfaces) = if_addrs::get_if_addrs() {
        for interface in interfaces {
            if let if_addrs::IfAddr::V4(address) = interface.addr {
                if let Some(broadcast) = address.broadcast {
                    targets.push(SocketAddr::new(IpAddr::V4(broadcast), DISCOVERY_PORT));
                }
            }
        }
    }
    targets.sort();
    targets.dedup();
    targets
}

fn add_peer(peers: &mut std::collections::HashMap<String, TransferPeer>, peer: TransferPeer) {
    if peers
        .values()
        .any(|existing| existing.host == peer.host && existing.port == peer.port)
    {
        return;
    }
    peers.insert(peer.id.clone(), peer);
}

fn drain_discovery(
    socket: &UdpSocket,
    peers: &mut std::collections::HashMap<String, TransferPeer>,
) {
    let mut buffer = [0u8; 1024];
    loop {
        match socket.recv_from(&mut buffer) {
            Ok((size, source)) => {
                let Ok(reply) = serde_json::from_slice::<DiscoveryReply>(&buffer[..size]) else {
                    continue;
                };
                if reply.v != 1 || reply.port == 0 || reply.name.trim().is_empty() {
                    continue;
                }
                add_peer(
                    peers,
                    TransferPeer {
                        id: format!("udp:{}:{}", source.ip(), reply.port),
                        name: reply.name,
                        host: source.ip().to_string(),
                        port: reply.port,
                    },
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(_) => break,
        }
    }
}

#[tauri::command]
pub fn transfer_receive_start(
    destination: String,
    replace_current: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> ApiResult<ReceiveStarted> {
    state.transfer.stop();

    #[cfg(not(target_os = "android"))]
    {
        let destination_path = state.fs_scope.resolve(&destination, Access::Write)?;
        if replace_current {
            if !destination_path.join("pms.vault").is_file() {
                return Err(ApiError::new(
                    "transfer_destination",
                    "Текущее хранилище не найдено",
                ));
            }
        } else if destination_path.exists()
            && fs::read_dir(&destination_path)
                .map_err(|error| ApiError::new("transfer_destination", error.to_string()))?
                .next()
                .is_some()
        {
            return Err(ApiError::new(
                "transfer_destination",
                "Выберите пустую папку для нового хранилища",
            ));
        }
    }
    #[cfg(target_os = "android")]
    {
        let _ = (&destination, replace_current);
    }

    let temp_dir = transfer_temp_dir(&app)?;
    // Prefer the LAN address: the receiver is not reachable from the internet
    // anyway (no port forwarding), but binding one interface is still tighter
    // than all of them.
    let bind = local_address()
        .map(|address| format!("{address}:0"))
        .unwrap_or_else(|_| "0.0.0.0:0".to_string());
    let server = Arc::new(
        Server::http(bind.as_str())
            .map_err(|error| ApiError::new("transfer_server", error.to_string()))?,
    );
    let port = server
        .server_addr()
        .to_ip()
        .ok_or_else(|| ApiError::new("transfer_server", "Не удалось определить порт"))?
        .port();
    let name = device_name();
    let host = local_address()?.to_string();
    let mdns = register_service(port, &format!("PMS · {name}"))
        .inspect_err(|error| {
            eprintln!("PMS transfer: mDNS unavailable: {}", error.message);
        })
        .ok();
    let code = generate_code();
    let stop = Arc::new(AtomicBool::new(false));
    let discovery = start_discovery_responder(port, &format!("PMS · {name}"), stop.clone()).ok();
    if discovery.is_none() {
        eprintln!("PMS transfer: UDP discovery responder is unavailable");
    }
    let stop_thread = stop.clone();
    let server_thread = server.clone();
    let thread_code = code.clone();
    let thread_destination = destination.clone();
    let thread_temp_dir = temp_dir;

    std::thread::spawn(move || {
        let deadline = Instant::now() + RECEIVE_TTL;
        let mut attempts = 0u32;
        while !stop_thread.load(Ordering::Relaxed) {
            if Instant::now() > deadline {
                // The code is short-lived on purpose: a code left on screen for
                // an hour is a code someone else may use.
                let _ = app.emit("transfer://expired", ());
                stop_thread.store(true, Ordering::Relaxed);
                server_thread.unblock();
                break;
            }
            let mut request = match server_thread.recv_timeout(Duration::from_millis(500)) {
                Ok(Some(request)) => request,
                Ok(None) | Err(_) => continue,
            };

            let path = request.url().to_string();
            if request.method() != &Method::Post || path != "/upload" {
                let _ = request.respond(Response::from_string("path").with_status_code(404));
                continue;
            }
            let header_value = |request: &tiny_http::Request, key: &str| {
                request
                    .headers()
                    .iter()
                    .find(|header| header.field.as_str().as_str().eq_ignore_ascii_case(key))
                    .map(|header| header.value.as_str().to_string())
                    .unwrap_or_default()
            };
            if !code_matches(&thread_code, &header_value(&request, "X-PMS-Code")) {
                attempts += 1;
                std::thread::sleep(Duration::from_millis(250));
                let _ = request
                    .respond(Response::from_string("code").with_status_code(403));
                if attempts >= MAX_CODE_ATTEMPTS {
                    let _ = app.emit("transfer://locked", attempts);
                    stop_thread.store(true, Ordering::Relaxed);
                    server_thread.unblock();
                    break;
                }
                continue;
            }
            let sender = header_value(&request, "X-PMS-Name");
            let expected_mac = header_value(&request, "X-PMS-Mac");
            let temp = thread_temp_dir.join(format!("pms-transfer-{}.pmsvault", Uuid::new_v4()));
            let mut output = match File::create(&temp) {
                Ok(file) => {
                    secure_file(&temp);
                    file
                }
                Err(_) => {
                    let _ = request.respond(Response::from_string("storage").with_status_code(500));
                    continue;
                }
            };
            let mut reader = request.as_reader().take(MAX_ARCHIVE + 1);
            let copied = std::io::copy(&mut reader, &mut output);
            let size = output.metadata().map(|meta| meta.len()).unwrap_or(0);
            drop(output);
            if copied.is_err() || size == 0 || size > MAX_ARCHIVE {
                let _ = fs::remove_file(&temp);
                let _ = request.respond(Response::from_string("size").with_status_code(413));
                continue;
            }
            // The MAC covers a digest of the body: a receiver that does not know
            // the code never accepts (or keeps) an archive, and a captured upload
            // cannot be replayed against a different receiver.
            let digest = match sha256_file(&temp) {
                Ok(digest) => digest,
                Err(_) => {
                    let _ = fs::remove_file(&temp);
                    let _ = request.respond(Response::from_string("io").with_status_code(500));
                    continue;
                }
            };
            let digest_bytes = hex::decode(&digest).unwrap_or_default();
            if expected_mac.is_empty() || body_mac(&thread_code, &digest_bytes) != expected_mac {
                attempts += 1;
                let _ = fs::remove_file(&temp);
                let _ = request
                    .respond(Response::from_string("mac").with_status_code(403));
                if attempts >= MAX_CODE_ATTEMPTS {
                    let _ = app.emit("transfer://locked", attempts);
                    stop_thread.store(true, Ordering::Relaxed);
                    server_thread.unblock();
                    break;
                }
                continue;
            }
            let _ = request.respond(Response::from_string("ok").with_status_code(StatusCode(200)));
            let _ = app.emit(
                "transfer://received",
                ReceivedArchive {
                    archive_path: temp.to_string_lossy().into_owned(),
                    sender: if sender.is_empty() {
                        "PMS".to_string()
                    } else {
                        sender
                    },
                    destination: thread_destination.clone(),
                    replace_current,
                },
            );
            stop_thread.store(true, Ordering::Relaxed);
            server_thread.unblock();
            break;
        }
    });

    if let Ok(mut guard) = state.transfer.receiver.lock() {
        *guard = Some(ReceiverSession {
            stop,
            server,
            mdns,
            discovery,
        });
    }

    Ok(ReceiveStarted {
        code,
        device_name: name,
        host,
        port,
    })
}

#[tauri::command]
pub fn transfer_receive_stop(state: State<'_, AppState>) -> ApiResult<()> {
    state.transfer.stop();
    Ok(())
}

#[tauri::command]
pub async fn transfer_scan(duration_ms: Option<u64>) -> ApiResult<Vec<TransferPeer>> {
    tauri::async_runtime::spawn_blocking(move || scan_peers(duration_ms))
        .await
        .map_err(|error| ApiError::new("transfer_scan", error.to_string()))?
}

fn scan_peers(duration_ms: Option<u64>) -> ApiResult<Vec<TransferPeer>> {
    let mdns = ServiceDaemon::new().ok();
    let receiver = mdns
        .as_ref()
        .and_then(|daemon| daemon.browse(SERVICE_TYPE).ok());
    if mdns.is_none() || receiver.is_none() {
        eprintln!("PMS transfer: mDNS unavailable, using UDP broadcast only");
    }
    let discovery = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok();
    if let Some(socket) = &discovery {
        let _ = socket.set_broadcast(true);
        let _ = socket.set_nonblocking(true);
    }
    let deadline =
        Instant::now() + Duration::from_millis(duration_ms.unwrap_or(4000).clamp(1000, 6000));
    let mut peers = std::collections::HashMap::new();
    let mut next_broadcast = Instant::now();

    while Instant::now() < deadline {
        if let Some(receiver) = &receiver {
            while let Ok(ServiceEvent::ServiceResolved(service)) = receiver.try_recv() {
                let Some(host) = service.get_addresses_v4().iter().next().copied() else {
                    continue;
                };
                let name = service
                    .get_property_val_str("name")
                    .unwrap_or(service.get_fullname())
                    .to_string();
                add_peer(
                    &mut peers,
                    TransferPeer {
                        id: service.get_fullname().to_string(),
                        name,
                        host: host.to_string(),
                        port: service.get_port(),
                    },
                );
            }
        }

        if let Some(socket) = &discovery {
            if Instant::now() >= next_broadcast {
                for target in discovery_targets() {
                    let _ = socket.send_to(DISCOVERY_REQUEST, target);
                }
                next_broadcast = Instant::now() + Duration::from_millis(900);
            }
            drain_discovery(socket, &mut peers);
        }

        std::thread::sleep(Duration::from_millis(80));
    }
    if let Some(mdns) = mdns {
        let _ = mdns.shutdown();
    }
    let mut peers = peers.into_values().collect::<Vec<_>>();
    peers.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(peers)
}

#[tauri::command]
pub async fn transfer_send(
    app: AppHandle,
    host: String,
    port: u16,
    code: String,
    vault_path: String,
    state: State<'_, AppState>,
) -> ApiResult<()> {
    let temp_dir = transfer_temp_dir(&app)?;
    let vault_path = state.fs_scope.resolve(&vault_path, Access::Read)?;
    tauri::async_runtime::spawn_blocking(move || {
        send_archive(host, port, code, vault_path.to_string_lossy().into_owned(), temp_dir)
    })
    .await
    .map_err(|error| ApiError::new("transfer_send", error.to_string()))?
}

fn send_archive(
    host: String,
    port: u16,
    code: String,
    vault_path: String,
    temp_dir: PathBuf,
) -> ApiResult<()> {
    let code = normalize_code(&code).ok_or_else(|| {
        ApiError::new(
            "transfer_code",
            "Код состоит из 8 символов латиницы и цифр (регистр и дефисы не важны)",
        )
    })?;
    let peer = format!("{host}:{port}");
    let address: SocketAddr = peer
        .parse()
        .map_err(|_| ApiError::new("transfer_network", "Некорректный адрес устройства"))?;
    let archive = temp_dir.join(format!("pms-send-{}.pmsvault", Uuid::new_v4()));
    pms_archive::export(Path::new(&vault_path), &archive).map_err(ApiError::from)?;
    secure_file(&archive);
    let result = (|| -> ApiResult<()> {
        let size = fs::metadata(&archive)
            .map_err(|error| ApiError::new("transfer_network", error.to_string()))?
            .len();
        let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(10))
            .map_err(|error| ApiError::new("transfer_network", error.to_string()))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(600)))
            .map_err(|error| ApiError::new("transfer_network", error.to_string()))?;
        let name = header_safe(&device_name());
        let digest = sha256_file(&archive)?;
        let mac = body_mac(&code, &hex::decode(&digest).unwrap_or_default());
        write!(
            stream,
            "POST /upload HTTP/1.1\r\nHost: {peer}\r\nX-PMS-Code: {code}\r\nX-PMS-Mac: {mac}\r\nX-PMS-Name: {name}\r\nContent-Type: application/octet-stream\r\nContent-Length: {size}\r\nConnection: close\r\n\r\n"
        )
        .map_err(|error| ApiError::new("transfer_network", error.to_string()))?;
        let mut file = File::open(&archive)
            .map_err(|error| ApiError::new("transfer_network", error.to_string()))?;
        std::io::copy(&mut file, &mut stream)
            .map_err(|error| ApiError::new("transfer_network", error.to_string()))?;
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .map_err(|error| ApiError::new("transfer_network", error.to_string()))?;
        let status_line = response.lines().next().unwrap_or("").trim().to_string();
        if !status_line.starts_with("HTTP/1.1 200") && !status_line.starts_with("HTTP/1.0 200") {
            let status = status_line
                .split_whitespace()
                .nth(1)
                .and_then(|value| value.parse::<u16>().ok());
            let message = match status {
                Some(403) => {
                    "Неверный код подтверждения. Проверьте код на принимающем устройстве."
                }
                Some(404) => {
                    "Принимающее устройство не приняло запрос. Обновите список устройств и попробуйте снова."
                }
                Some(413) => {
                    "Принимающее устройство не смогло принять архив: он слишком большой или не записался."
                }
                Some(500) => {
                    "Принимающее устройство не смогло сохранить архив. Проверьте свободное место."
                }
                _ => "Принимающее устройство отклонило передачу.",
            };
            let body = response.split("\r\n\r\n").nth(1).unwrap_or("").trim();
            eprintln!("PMS transfer: upload rejected peer={peer} status={status_line} body={body}");
            return Err(ApiError::new("transfer_rejected", message));
        }
        Ok(())
    })();
    let _ = fs::remove_file(&archive);
    result
}

#[tauri::command]
pub async fn transfer_install(
    archive_path: String,
    destination: String,
    replace_current: bool,
    state: State<'_, AppState>,
) -> ApiResult<String> {
    let archive_path = state.fs_scope.resolve(&archive_path, Access::Read)?;
    // Either a replacement inside the current vault folder or a fresh folder the
    // user picked for the new vault: both must be writable grants.
    let destination = state.fs_scope.resolve(&destination, Access::Write)?;
    let archive_path = archive_path.to_string_lossy().into_owned();
    let destination = destination.to_string_lossy().into_owned();
    tauri::async_runtime::spawn_blocking(move || {
        install_archive(archive_path, destination, replace_current)
    })
    .await
    .map_err(|error| ApiError::new("transfer_install", error.to_string()))?
}

struct RemoveOnDrop<'a>(&'a Path);

impl Drop for RemoveOnDrop<'_> {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.0);
    }
}

fn install_archive(
    archive_path: String,
    destination: String,
    replace_current: bool,
) -> ApiResult<String> {
    let archive = Path::new(&archive_path);
    let _cleanup = RemoveOnDrop(archive);
    let destination = Path::new(&destination);
    if replace_current {
        secure_dir(destination);
        let backup_dir = destination.join("backups");
        fs::create_dir_all(&backup_dir)
            .map_err(|error| ApiError::new("transfer_install", error.to_string()))?;
        let current = destination.join("pms.vault");
        if current.is_file() {
            let name = format!(
                "{}.vault",
                chrono::Utc::now()
                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
                    .replace([':', '.', '-'], "")
            );
            let backup = backup_dir.join(name);
            fs::copy(&current, &backup)
                .map_err(|error| ApiError::new("transfer_install", error.to_string()))?;
            secure_file(&backup);
        }
        let staging = tempfile::tempdir_in(
            destination
                .parent()
                .ok_or_else(|| ApiError::new("transfer_install", "Некорректная папка"))?,
        )
        .map_err(|error| ApiError::new("transfer_install", error.to_string()))?;
        let staged = staging.path().join("incoming");
        pms_archive::import(archive, &staged).map_err(ApiError::from)?;
        fs::copy(staged.join("pms.vault"), destination.join("pms.vault"))
            .map_err(|error| ApiError::new("transfer_install", error.to_string()))?;
        secure_file(&destination.join("pms.vault"));
        let attachments = destination.join("attachments");
        fs::create_dir_all(&attachments)
            .map_err(|error| ApiError::new("transfer_install", error.to_string()))?;
        secure_dir(&attachments);
        if let Ok(entries) = fs::read_dir(staged.join("attachments")) {
            for entry in entries.flatten() {
                if entry.path().is_file() {
                    let target = attachments.join(entry.file_name());
                    let _ = fs::copy(entry.path(), &target);
                    secure_file(&target);
                }
            }
        }
    } else {
        pms_archive::import(archive, destination).map_err(ApiError::from)?;
        secure_dir(destination);
        secure_file(&destination.join("pms.vault"));
        secure_dir(&destination.join("attachments"));
        if let Ok(entries) = fs::read_dir(destination.join("attachments")) {
            for entry in entries.flatten() {
                if entry.path().is_file() {
                    secure_file(&entry.path());
                }
            }
        }
    }
    Ok(destination.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_long_random_and_forgiving_to_type() {
        let code = generate_code();
        assert_eq!(code.len(), CODE_LEN);
        assert!(code.bytes().all(|byte| CODE_ALPHABET.contains(&byte)));
        assert_ne!(code, generate_code());

        // Case, spaces and dashes do not matter when the user retypes it.
        let spaced = format!("{}-{}", &code[..4], &code[4..]);
        assert_eq!(normalize_code(&spaced).as_deref(), Some(code.as_str()));
        assert_eq!(
            normalize_code(&code.to_lowercase()).as_deref(),
            Some(code.as_str())
        );
        // Ambiguous characters and wrong length are refused.
        assert_eq!(normalize_code("IIIIOOOO"), None);
        assert_eq!(normalize_code("ABC"), None);
        assert!(code_matches(&code, &spaced));
        assert!(!code_matches(&code, "ABCDEFGH"));
        assert!(!code_matches(&code, ""));
    }

    #[test]
    fn the_upload_mac_binds_the_code_and_the_body() {
        let code = "ABCD2345";
        let digest = [7u8; 32];
        let mac = body_mac(code, &digest);
        assert_eq!(mac.len(), 64);
        assert_eq!(body_mac(code, &digest), mac, "HMAC is deterministic");
        assert_ne!(body_mac("ABCD2346", &digest), mac, "code is part of the key");
        assert_ne!(body_mac(code, &[8u8; 32]), mac, "body digest is covered");
    }

    #[test]
    #[ignore]
    fn udp_discovery_finds_local_responder() {
        let responder = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, DISCOVERY_PORT)).unwrap();
        responder
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 256];
            if let Ok((_, sender)) = responder.recv_from(&mut buffer) {
                let reply = DiscoveryReply {
                    v: 1,
                    name: "PMS · UDP test".to_string(),
                    port: 41234,
                };
                let _ = responder.send_to(&serde_json::to_vec(&reply).unwrap(), sender);
            }
        });
        let peers = scan_peers(Some(2000)).unwrap();
        assert!(peers.iter().any(|peer| peer.name == "PMS · UDP test"));
    }

    fn make_vault(folder: &Path, vault: &[u8], attachment: Option<(&str, &[u8])>) {
        fs::create_dir_all(folder).unwrap();
        fs::write(folder.join("pms.vault"), vault).unwrap();
        if let Some((name, bytes)) = attachment {
            fs::create_dir_all(folder.join("attachments")).unwrap();
            fs::write(folder.join("attachments").join(name), bytes).unwrap();
        }
    }

    #[test]
    fn install_new_archive_restores_vault_and_attachments() {
        let source = tempfile::tempdir().unwrap();
        make_vault(
            source.path(),
            b"encrypted-new",
            Some(("new.pmsa", b"new-attachment")),
        );
        let archive = source.path().join("transfer.pmsvault");
        pms_archive::export(source.path(), &archive).unwrap();

        let target = tempfile::tempdir().unwrap();
        let destination = target.path().join("received");
        install_archive(
            archive.to_string_lossy().into_owned(),
            destination.to_string_lossy().into_owned(),
            false,
        )
        .unwrap();

        assert_eq!(
            fs::read(destination.join("pms.vault")).unwrap(),
            b"encrypted-new"
        );
        assert_eq!(
            fs::read(destination.join("attachments/new.pmsa")).unwrap(),
            b"new-attachment"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(destination.join("pms.vault"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
        assert!(!archive.exists());
    }

    #[test]
    fn replace_archive_keeps_a_backup_and_existing_attachments() {
        let source = tempfile::tempdir().unwrap();
        make_vault(
            source.path(),
            b"encrypted-new",
            Some(("new.pmsa", b"new-attachment")),
        );
        let archive = source.path().join("transfer.pmsvault");
        pms_archive::export(source.path(), &archive).unwrap();

        let target = tempfile::tempdir().unwrap();
        let destination = target.path().join("current");
        make_vault(
            &destination,
            b"encrypted-old",
            Some(("old.pmsa", b"old-attachment")),
        );

        install_archive(
            archive.to_string_lossy().into_owned(),
            destination.to_string_lossy().into_owned(),
            true,
        )
        .unwrap();

        assert_eq!(
            fs::read(destination.join("pms.vault")).unwrap(),
            b"encrypted-new"
        );
        assert_eq!(
            fs::read(destination.join("attachments/old.pmsa")).unwrap(),
            b"old-attachment"
        );
        assert_eq!(
            fs::read(destination.join("attachments/new.pmsa")).unwrap(),
            b"new-attachment"
        );

        let backups = fs::read_dir(destination.join("backups"))
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read(&backups[0]).unwrap(), b"encrypted-old");
    }

    #[test]
    #[ignore]
    fn mdns_service_is_discoverable() {
        let daemon = register_service(41234, "PMS · mdns-test").unwrap();
        std::thread::sleep(Duration::from_millis(500));
        let peers = scan_peers(Some(4000)).unwrap();
        assert!(
            peers.iter().any(|peer| peer.name.contains("mdns-test")),
            "peers: {peers:?}"
        );
        daemon.shutdown().unwrap();
    }

    #[test]
    fn http_transfer_sends_the_archive_to_the_receiver() {
        let source = tempfile::tempdir().unwrap();
        make_vault(
            source.path(),
            b"encrypted-sent",
            Some(("a.pmsa", b"payload")),
        );

        let server = Arc::new(Server::http("127.0.0.1:0").unwrap());
        let port = server.server_addr().to_ip().unwrap().port();
        let (tx, rx) = std::sync::mpsc::channel();
        let server_thread = server.clone();
        std::thread::spawn(move || {
            let mut request = server_thread.recv().unwrap();
            let code = request
                .headers()
                .iter()
                .find(|header| {
                    header
                        .field
                        .as_str()
                        .as_str()
                        .eq_ignore_ascii_case("X-PMS-Code")
                })
                .map(|header| header.value.as_str().to_string());
            let mac = request
                .headers()
                .iter()
                .find(|header| {
                    header
                        .field
                        .as_str()
                        .as_str()
                        .eq_ignore_ascii_case("X-PMS-Mac")
                })
                .map(|header| header.value.as_str().to_string());
            let mut body = Vec::new();
            request.as_reader().read_to_end(&mut body).unwrap();
            let _ = request.respond(Response::from_string("ok").with_status_code(200));
            tx.send((code, mac, body)).unwrap();
        });

        let temp = tempfile::tempdir().unwrap();
        send_archive(
            "127.0.0.1".to_string(),
            port,
            "ABCD2345".to_string(),
            source.path().to_string_lossy().into_owned(),
            temp.path().to_path_buf(),
        )
        .unwrap();

        let (code, request_mac, body) = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(code.as_deref(), Some("ABCD2345"));
        // The MAC proves knowledge of the code without sending it in the clear.
        let mac = request_mac.unwrap_or_default();
        assert_eq!(mac.len(), 64);
        use sha2::Digest as _;
        let digest = sha2::Sha256::digest(&body);
        assert_eq!(mac, body_mac("ABCD2345", &digest));
        let archive = source.path().join("received.pmsvault");
        fs::write(&archive, body).unwrap();
        let target = tempfile::tempdir().unwrap();
        let destination = target.path().join("restored");
        pms_archive::import(&archive, &destination).unwrap();
        assert_eq!(
            fs::read(destination.join("pms.vault")).unwrap(),
            b"encrypted-sent"
        );
    }

    #[test]
    fn failed_install_removes_the_received_archive() {
        let source = tempfile::tempdir().unwrap();
        let archive = source.path().join("bad.pmsvault");
        fs::write(&archive, b"not-a-zip").unwrap();
        let target = tempfile::tempdir().unwrap();
        let destination = target.path().join("restored");

        let error = install_archive(
            archive.to_string_lossy().into_owned(),
            destination.to_string_lossy().into_owned(),
            false,
        )
        .unwrap_err();

        assert_eq!(error.code, "import_corrupt");
        assert!(!archive.exists());
    }

    #[test]
    fn http_transfer_reports_a_rejected_code() {
        let source = tempfile::tempdir().unwrap();
        make_vault(source.path(), b"encrypted-sent", None);

        let server = Arc::new(Server::http("127.0.0.1:0").unwrap());
        let port = server.server_addr().to_ip().unwrap().port();
        let server_thread = server.clone();
        std::thread::spawn(move || {
            let mut request = server_thread.recv().unwrap();
            let mut body = Vec::new();
            request.as_reader().read_to_end(&mut body).unwrap();
            let _ = request
                .respond(Response::from_string("forbidden").with_status_code(StatusCode(403)));
        });

        let temp = tempfile::tempdir().unwrap();
        let error = send_archive(
            "127.0.0.1".to_string(),
            port,
            "ZZZZ9999".to_string(),
            source.path().to_string_lossy().into_owned(),
            temp.path().to_path_buf(),
        )
        .unwrap_err();

        assert_eq!(error.code, "transfer_rejected");
        assert!(error.message.contains("Неверный код"), "{}", error.message);
    }

    #[test]
    fn http_transfer_reports_a_receiver_storage_failure() {
        let source = tempfile::tempdir().unwrap();
        make_vault(source.path(), b"encrypted-sent", None);

        let server = Arc::new(Server::http("127.0.0.1:0").unwrap());
        let port = server.server_addr().to_ip().unwrap().port();
        let server_thread = server.clone();
        std::thread::spawn(move || {
            let mut request = server_thread.recv().unwrap();
            let mut body = Vec::new();
            request.as_reader().read_to_end(&mut body).unwrap();
            let _ =
                request.respond(Response::from_string("storage").with_status_code(StatusCode(500)));
        });

        let temp = tempfile::tempdir().unwrap();
        let error = send_archive(
            "127.0.0.1".to_string(),
            port,
            "ABCD2345".to_string(),
            source.path().to_string_lossy().into_owned(),
            temp.path().to_path_buf(),
        )
        .unwrap_err();

        assert_eq!(error.code, "transfer_rejected");
        assert!(
            error.message.contains("не смогло сохранить архив"),
            "{}",
            error.message
        );
    }
}
