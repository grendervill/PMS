use std::fs::{self, File};
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::path::Path;
#[cfg(not(target_os = "android"))]
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use rand::Rng;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tiny_http::{Method, Response, Server, StatusCode};
use uuid::Uuid;

use crate::commands::{ApiError, ApiResult, AppState};
use crate::core::pms_archive;

const SERVICE_TYPE: &str = "_pms._tcp.local.";
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
    local_ip_address::local_ip()
        .map_err(|error| ApiError::new("transfer_network", error.to_string()))
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
        let destination_path = PathBuf::from(&destination);
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

    let server = Arc::new(
        Server::http("0.0.0.0:0")
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
    let code = format!("{:06}", rand::thread_rng().gen_range(0..1_000_000));
    let stop = Arc::new(AtomicBool::new(false));
    let discovery = start_discovery_responder(port, &format!("PMS · {name}"), stop.clone()).ok();
    if discovery.is_none() {
        eprintln!("PMS transfer: UDP discovery responder is unavailable");
    }
    let stop_thread = stop.clone();
    let server_thread = server.clone();
    let thread_code = code.clone();
    let thread_destination = destination.clone();

    std::thread::spawn(move || {
        while !stop_thread.load(Ordering::Relaxed) {
            let mut request = match server_thread.recv_timeout(Duration::from_millis(500)) {
                Ok(Some(request)) => request,
                Ok(None) | Err(_) => continue,
            };

            let path = request.url().to_string();
            if request.method() != &Method::Post || path != "/upload" {
                let _ = request.respond(Response::from_string("not found").with_status_code(404));
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
            if header_value(&request, "X-PMS-Code") != thread_code {
                std::thread::sleep(Duration::from_millis(250));
                let _ = request.respond(Response::from_string("forbidden").with_status_code(403));
                continue;
            }
            let sender = header_value(&request, "X-PMS-Name");
            let temp =
                std::env::temp_dir().join(format!("pms-transfer-{}.pmsvault", Uuid::new_v4()));
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
                let _ = request.respond(Response::from_string("too large").with_status_code(413));
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
    host: String,
    port: u16,
    code: String,
    vault_path: String,
) -> ApiResult<()> {
    tauri::async_runtime::spawn_blocking(move || send_archive(host, port, code, vault_path))
        .await
        .map_err(|error| ApiError::new("transfer_send", error.to_string()))?
}

fn send_archive(host: String, port: u16, code: String, vault_path: String) -> ApiResult<()> {
    let peer = format!("{host}:{port}");
    let address: SocketAddr = peer
        .parse()
        .map_err(|_| ApiError::new("transfer_network", "Некорректный адрес устройства"))?;
    let archive = std::env::temp_dir().join(format!("pms-send-{}.pmsvault", Uuid::new_v4()));
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
        write!(
            stream,
            "POST /upload HTTP/1.1\r\nHost: {peer}\r\nX-PMS-Code: {code}\r\nX-PMS-Name: {name}\r\nContent-Type: application/octet-stream\r\nContent-Length: {size}\r\nConnection: close\r\n\r\n"
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
        if !response.starts_with("HTTP/1.1 200") && !response.starts_with("HTTP/1.0 200") {
            return Err(ApiError::new(
                "transfer_rejected",
                "Принимающее устройство отклонило передачу",
            ));
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
) -> ApiResult<String> {
    tauri::async_runtime::spawn_blocking(move || {
        install_archive(archive_path, destination, replace_current)
    })
    .await
    .map_err(|error| ApiError::new("transfer_install", error.to_string()))?
}

fn install_archive(
    archive_path: String,
    destination: String,
    replace_current: bool,
) -> ApiResult<String> {
    let archive = Path::new(&archive_path);
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
    let _ = fs::remove_file(archive);
    Ok(destination.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

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
            let mut body = Vec::new();
            request.as_reader().read_to_end(&mut body).unwrap();
            let _ = request.respond(Response::from_string("ok").with_status_code(200));
            tx.send((code, body)).unwrap();
        });

        send_archive(
            "127.0.0.1".to_string(),
            port,
            "123456".to_string(),
            source.path().to_string_lossy().into_owned(),
        )
        .unwrap();

        let (code, body) = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(code.as_deref(), Some("123456"));
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
}
