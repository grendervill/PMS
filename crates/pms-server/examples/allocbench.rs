//! Аллокации на серверном пути: запросы идут прямо в роутер, без сокетов и TLS,
//! поэтому счётчик показывает работу самого сервера.
//!
//! ```sh
//! cargo run -p pms-server --release --example allocbench
//! ```

use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

static ON: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static REALLOCS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ON.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        unsafe { mimalloc::MiMalloc.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if ON.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        unsafe { mimalloc::MiMalloc.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { mimalloc::MiMalloc.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if ON.load(Ordering::Relaxed) {
            REALLOCS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        }
        unsafe { mimalloc::MiMalloc.realloc(pointer, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn reset() {
    ALLOCS.store(0, Ordering::Relaxed);
    REALLOCS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
}

fn snapshot() -> (u64, u64, u64) {
    (
        ALLOCS.load(Ordering::Relaxed),
        REALLOCS.load(Ordering::Relaxed),
        BYTES.load(Ordering::Relaxed),
    )
}

fn rss_peak_mib() -> f64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|line| line.starts_with("VmHWM:"))
                .and_then(|line| line.split_whitespace().nth(1)?.parse::<f64>().ok())
        })
        .map(|kib| kib / 1024.0)
        .unwrap_or(0.0)
}

/// Средние аллокации на запрос: три прогрева (кэш подготовленных запросов,
/// страницы SQLite), затем `runs` замеров и деление на их число.
async fn bench(
    app: &axum::Router,
    runs: u64,
    mut make: impl FnMut() -> Request<Body>,
) -> (u64, u64, u64) {
    for _ in 0..3 {
        let _ = call(app, make()).await;
    }

    reset();
    ON.store(true, Ordering::Relaxed);
    for _ in 0..runs {
        let _ = call(app, make()).await;
    }
    ON.store(false, Ordering::Relaxed);

    let (allocs, reallocs, bytes) = snapshot();
    (allocs / runs, reallocs / runs, bytes / runs)
}

/// То же, но тело ответа не читается: считаем только работу сервера до ответа.
async fn bench_headers(
    app: &axum::Router,
    runs: u64,
    mut make: impl FnMut() -> Request<Body>,
) -> (u64, u64, u64) {
    for _ in 0..3 {
        let response = app.clone().oneshot(make()).await.expect("ответ");
        drop(response);
    }

    reset();
    ON.store(true, Ordering::Relaxed);
    for _ in 0..runs {
        let response = app.clone().oneshot(make()).await.expect("ответ");
        drop(response);
    }
    ON.store(false, Ordering::Relaxed);

    let (allocs, reallocs, bytes) = snapshot();
    (allocs / runs, reallocs / runs, bytes / runs)
}

async fn call(app: &axum::Router, request: Request<Body>) -> (StatusCode, Vec<u8>) {
    let response = app.clone().oneshot(request).await.expect("ответ");
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("тело")
        .to_bytes()
        .to_vec();
    (status, body)
}

#[tokio::main]
async fn main() {
    let dir = tempfile::tempdir().expect("каталог");
    let config = pms_server::config::Config::parse(&format!(
        "[server]\ndata_dir = \"{}\"\nlisten = \"127.0.0.1:0\"\n[tls]\nrequired = false\nmode = \"off\"\n",
        dir.path().join("data").display()
    ))
    .expect("конфиг");

    let store = Arc::new(pms_server::store::Store::open(dir.path(), 1 << 30).expect("стор"));
    let auth = Arc::new(pms_server::auth::Auth::new(
        Arc::clone(&store),
        config.auth.clone(),
    ));
    let app = pms_server::api::router(pms_server::api::AppState::new(
        Arc::clone(&store),
        auth,
        Arc::new(config),
    ));

    // Регистрация: аккаунт с пустым сейфом.
    let identity = pms_proto::identity::derive_identity(
        b"alloc-bench",
        "bench",
        &pms_proto::identity::IdentityKdf::FAST_FOR_TESTS,
    );
    let account = pms_proto::identity::AccountId::from_public_key(&identity.public);
    let device = pms_proto::identity::DeviceId::new([7u8; 16]);
    let digest = *blake3::hash(b"").as_bytes();
    let signature = pms_proto::identity::sign_register(&identity, &account, &device, &digest);

    reset();
    ON.store(true, Ordering::Relaxed);
    let (status, _) = call(
        &app,
        Request::builder()
            .method("POST")
            .uri("/v1/accounts")
            .header("x-pms-protocol", "1")
            .header("x-pms-account-id", account.to_string())
            .header("x-pms-public-key", hex::encode(identity.public))
            .header("x-pms-login", "bench")
            .header("x-pms-device-id", device.to_string())
            .header("x-pms-signature", hex::encode(signature))
            .body(Body::from(Vec::new()))
            .expect("запрос"),
    )
    .await;
    ON.store(false, Ordering::Relaxed);
    let register_stats = snapshot();
    assert_eq!(status, StatusCode::CREATED, "регистрация");

    // Вход: challenge + verify.
    let challenge_request = || {
        Request::builder()
            .method("POST")
            .uri("/v1/auth/challenge")
            .header("content-type", "application/json")
            .header("x-pms-protocol", "1")
            .body(Body::from(
                serde_json::json!({"login": "bench", "deviceId": device.to_string()}).to_string(),
            ))
            .expect("запрос")
    };

    reset();
    ON.store(true, Ordering::Relaxed);
    let (_, body) = call(&app, challenge_request()).await;
    ON.store(false, Ordering::Relaxed);
    let challenge_stats = snapshot();
    let challenge: serde_json::Value = serde_json::from_slice(&body).expect("challenge");

    let nonce = hex::decode(challenge["nonce"].as_str().expect("nonce")).expect("hex");
    let mut nonce_bytes = [0u8; 32];
    nonce_bytes.copy_from_slice(&nonce);
    let signature = pms_proto::identity::sign_challenge(&identity, &account, &device, &nonce_bytes);

    reset();
    ON.store(true, Ordering::Relaxed);
    let (status, body) = call(
        &app,
        Request::builder()
            .method("POST")
            .uri("/v1/auth/verify")
            .header("content-type", "application/json")
            .header("x-pms-protocol", "1")
            .body(Body::from(
                serde_json::json!({
                    "accountId": account.to_string(),
                    "deviceId": device.to_string(),
                    "nonce": challenge["nonce"].clone(),
                    "signature": hex::encode(signature),
                })
                .to_string(),
            ))
            .expect("запрос"),
    )
    .await;
    ON.store(false, Ordering::Relaxed);
    let verify_stats = snapshot();
    assert_eq!(status, StatusCode::OK, "вход");
    let verified: serde_json::Value = serde_json::from_slice(&body).expect("verify");
    let token = verified["token"].as_str().expect("token").to_string();

    let mut rows: Vec<(String, u64, u64, u64)> = vec![
        (
            "POST /v1/accounts".to_string(),
            register_stats.0,
            register_stats.1,
            register_stats.2,
        ),
        (
            "POST /v1/auth/challenge".to_string(),
            challenge_stats.0,
            challenge_stats.1,
            challenge_stats.2,
        ),
        (
            "POST /v1/auth/verify".to_string(),
            verify_stats.0,
            verify_stats.1,
            verify_stats.2,
        ),
    ];

    // Сохранения сейфа разных размеров и одно чтение: по 20 замеров на запрос.
    for (label, size) in [
        ("PUT /v1/vault 2 КиБ", 2 * 1024usize),
        ("PUT /v1/vault 64 КиБ", 64 * 1024),
        ("PUT /v1/vault 1 МиБ", 1024 * 1024),
    ] {
        let payload = axum::body::Bytes::from(vec![0xABu8; size]);
        let token = token.clone();
        let stats = bench(&app, 20, move || {
            Request::builder()
                .method("PUT")
                .uri("/v1/vault")
                .header("x-pms-protocol", "1")
                .header("authorization", format!("Bearer {token}"))
                .header("if-match", "*")
                .header("content-length", size.to_string())
                .body(Body::from(payload.clone()))
                .expect("запрос")
        })
        .await;
        rows.push((label.to_string(), stats.0, stats.1, stats.2));
    }

    let token = token.clone();
    let stats = bench_headers(&app, 20, move || {
        Request::builder()
            .uri("/v1/vault")
            .header("x-pms-protocol", "1")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .expect("запрос")
    })
    .await;
    rows.push((
        "GET /v1/vault (тело не считается)".to_string(),
        stats.0,
        stats.1,
        stats.2,
    ));

    let stats = bench(&app, 50, || {
        Request::builder()
            .uri("/v1/healthz")
            .body(Body::empty())
            .expect("запрос")
    })
    .await;
    rows.push(("GET /v1/healthz".to_string(), stats.0, stats.1, stats.2));

    println!(
        "{:<26} {:>8} {:>9} {:>10}",
        "запрос", "alloc", "realloc", "байт"
    );
    for (label, allocs, reallocs, bytes) in rows {
        println!("{label:<26} {allocs:>8} {reallocs:>9} {bytes:>10}");
    }
    println!("\nпик RSS процесса: {:.1} МиБ", rss_peak_mib());

    // Прямой путь хранилища: сколько стоит запись без HTTP.
    let body = vec![0xCDu8; 64 * 1024];
    reset();
    ON.store(true, Ordering::Relaxed);
    store
        .put_vault(
            account,
            pms_server::store::vault::ExpectedVersion::Any,
            body.as_slice(),
            body.len() as u64,
        )
        .await
        .expect("запись");
    ON.store(false, Ordering::Relaxed);
    let stats = snapshot();
    println!(
        "Store::put_vault 64 КиБ      {:>8} {:>9} {:>10}",
        stats.0, stats.1, stats.2
    );
}
