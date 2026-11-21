//! Нагрузочный тест сервера: регистрация многих пользователей и одновременные
//! сохранения сейфов.
//!
//! ```sh
//! cargo run -p pms-server --release --example loadtest -- \
//!     --url https://192.168.1.233:8443 --users 120 --pushes 20 --concurrency 16
//! ```
//!
//! Инструмент ходит по TLS без проверки цепочки (сервер тестовый) и выводит
//! задержки и ошибки, чтобы их можно было сопоставить с `docker stats`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use pms_proto::dto::{
    ChallengeRequest, ChallengeResponse, VaultMeta, VerifyRequest, VerifyResponse,
};
use pms_proto::identity::{
    derive_identity, sign_challenge, sign_register, AccountId, DeviceId, IdentityKdf,
};

#[derive(Debug)]
struct Options {
    url: String,
    users: usize,
    pushes: usize,
    concurrency: usize,
    prefix: String,
}

fn parse_args() -> Options {
    let mut options = Options {
        url: "https://127.0.0.1:8443".to_string(),
        users: 50,
        pushes: 10,
        concurrency: 8,
        prefix: "load".to_string(),
    };

    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = || args.next().expect("нужно значение флага");
        match flag.as_str() {
            "--url" => options.url = value(),
            "--users" => options.users = value().parse().expect("число пользователей"),
            "--pushes" => options.pushes = value().parse().expect("число сохранений"),
            "--concurrency" => options.concurrency = value().parse().expect("параллелизм"),
            "--prefix" => options.prefix = value(),
            other => panic!("неизвестный флаг {other}"),
        }
    }
    options
}

struct User {
    login: String,
    token: String,
}

async fn register(
    http: &reqwest::Client,
    base: &str,
    login: &str,
    device: DeviceId,
) -> Result<User, String> {
    // Параметры KDF в нагрузочном тесте быстрые: сервер их не проверяет, а Argon2id
    // по 128 МиБ на 120 пользователей — это тест памяти ноутбука, не сервера.
    let identity = Arc::new(derive_identity(
        format!("load-secret-{login}").as_bytes(),
        login,
        &IdentityKdf::FAST_FOR_TESTS,
    ));
    let account = AccountId::from_public_key(&identity.public);

    let vault = format!("vault-{login}").into_bytes();
    let digest = *blake3::hash(&vault).as_bytes();
    let signature = sign_register(&identity, &account, &device, &digest);

    let response = http
        .post(format!("{base}/v1/accounts"))
        .header("x-pms-protocol", "1")
        .header("x-pms-account-id", account.to_string())
        .header("x-pms-public-key", hex::encode(identity.public))
        .header("x-pms-login", login)
        .header("x-pms-device-id", device.to_string())
        .header("x-pms-signature", hex::encode(signature))
        .body(vault)
        .send()
        .await
        .map_err(|error| error.to_string())?;

    // Повторный прогон: аккаунт уже есть — это нормально.
    if !response.status().is_success() && response.status().as_u16() != 409 {
        return Err(format!(
            "регистрация {}: {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }

    let token = login_user(http, base, login, &identity, account, device).await?;
    Ok(User {
        login: login.to_string(),
        token,
    })
}

async fn login_user(
    http: &reqwest::Client,
    base: &str,
    login: &str,
    identity: &pms_proto::identity::Identity,
    account: AccountId,
    device: DeviceId,
) -> Result<String, String> {
    let challenge: ChallengeResponse = http
        .post(format!("{base}/v1/auth/challenge"))
        .header("x-pms-protocol", "1")
        .json(&ChallengeRequest {
            login: login.to_string(),
            device_id: device.to_string(),
        })
        .send()
        .await
        .map_err(|error| error.to_string())?
        .json()
        .await
        .map_err(|error| error.to_string())?;

    let mut nonce = [0u8; 32];
    nonce.copy_from_slice(&hex::decode(&challenge.nonce).map_err(|error| error.to_string())?);
    let signature = sign_challenge(identity, &account, &device, &nonce);

    let verified: VerifyResponse = http
        .post(format!("{base}/v1/auth/verify"))
        .header("x-pms-protocol", "1")
        .json(&VerifyRequest {
            account_id: account.to_string(),
            device_id: device.to_string(),
            nonce: challenge.nonce,
            signature: hex::encode(signature),
        })
        .send()
        .await
        .map_err(|error| error.to_string())?
        .json()
        .await
        .map_err(|error| error.to_string())?;

    Ok(verified.token)
}

fn percentile(values: &[u128], p: f64) -> u128 {
    if values.is_empty() {
        return 0;
    }
    let index = ((values.len() as f64 - 1.0) * p).round() as usize;
    values[index.min(values.len() - 1)]
}

#[tokio::main]
async fn main() {
    let options = parse_args();
    let http = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(30))
        .build()
        .expect("клиент");

    let base = options.url.trim_end_matches('/').to_string();
    println!(
        "сервер {} · {} пользователей · {} сохранений каждый · параллельно {}",
        base, options.users, options.pushes, options.concurrency
    );

    // 1. Регистрация и вход.
    let started = Instant::now();
    let mut users = Vec::with_capacity(options.users);
    let next = Arc::new(AtomicUsize::new(0));
    let errors = Arc::new(AtomicUsize::new(0));
    let mut tasks = Vec::new();

    for _ in 0..options.concurrency {
        let http = http.clone();
        let base = base.clone();
        let prefix = options.prefix.clone();
        let next = Arc::clone(&next);
        let errors = Arc::clone(&errors);
        let total = options.users;
        tasks.push(tokio::spawn(async move {
            let mut created = Vec::new();
            loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                if index >= total {
                    break;
                }
                let login = format!("{prefix}{index:03}");
                let device = DeviceId::new([(index % 250 + 1) as u8; 16]);
                match register(&http, &base, &login, device).await {
                    Ok(user) => created.push(user),
                    Err(error) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                        eprintln!("{login}: {error}");
                    }
                }
            }
            created
        }));
    }

    for task in tasks {
        users.extend(task.await.expect("задача"));
    }
    let register_ms = started.elapsed().as_millis();
    println!(
        "регистрация: {} аккаунтов за {} мс ({:.1}/с), ошибок {}",
        users.len(),
        register_ms,
        users.len() as f64 / (register_ms as f64 / 1000.0),
        errors.load(Ordering::Relaxed)
    );

    // 2. Одновременные сохранения: у каждого пользователя своя версия.
    let started = Instant::now();
    let latencies = Arc::new(std::sync::Mutex::new(Vec::<u128>::new()));
    let push_errors = Arc::new(AtomicUsize::new(0));
    let users = Arc::new(users);
    let next = Arc::new(AtomicUsize::new(0));
    let mut tasks = Vec::new();

    for _ in 0..options.concurrency {
        let http = http.clone();
        let base = base.clone();
        let users = Arc::clone(&users);
        let latencies = Arc::clone(&latencies);
        let push_errors = Arc::clone(&push_errors);
        let next = Arc::clone(&next);
        let pushes = options.pushes;
        tasks.push(tokio::spawn(async move {
            loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                if index >= users.len() {
                    break;
                }
                let user = &users[index];
                for round in 0..pushes {
                    let body = format!("{}|round-{round}|{}", user.login, "x".repeat(2048));
                    let started = Instant::now();
                    let response = http
                        .put(format!("{base}/v1/vault"))
                        .header("x-pms-protocol", "1")
                        .bearer_auth(&user.token)
                        // После регистрации с телом сейфа версия уже 1.
                        .header("if-match", (round + 1).to_string())
                        .body(body.into_bytes())
                        .send()
                        .await;

                    match response {
                        Ok(response) if response.status().is_success() => {
                            latencies
                                .lock()
                                .expect("latencies")
                                .push(started.elapsed().as_millis());
                        }
                        Ok(response) => {
                            push_errors.fetch_add(1, Ordering::Relaxed);
                            let status = response.status();
                            let body = response.text().await.unwrap_or_default();
                            eprintln!("{} round {round}: PUT {status} {body}", user.login);
                        }
                        Err(error) => {
                            push_errors.fetch_add(1, Ordering::Relaxed);
                            eprintln!("{}: {error}", user.login);
                        }
                    }
                }
            }
        }));
    }

    for task in tasks {
        task.await.expect("задача");
    }

    let total_ms = started.elapsed().as_millis();
    let mut sorted = latencies.lock().expect("latencies").clone();
    sorted.sort_unstable();
    let total = sorted.len();
    println!(
        "сохранения: {} запросов за {} мс ({:.1}/с), ошибок {}",
        total,
        total_ms,
        total as f64 / (total_ms as f64 / 1000.0),
        push_errors.load(Ordering::Relaxed)
    );
    println!(
        "задержка PUT: p50 {} мс · p95 {} мс · p99 {} мс · max {} мс",
        percentile(&sorted, 0.50),
        percentile(&sorted, 0.95),
        percentile(&sorted, 0.99),
        sorted.last().copied().unwrap_or(0)
    );
    let _ = std::any::type_name::<VaultMeta>();
}
