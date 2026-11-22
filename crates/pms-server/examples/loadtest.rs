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
    /// Держать нагрузку столько секунд вместо фиксированного числа сохранений.
    duration: Option<u64>,
    /// Читать сейфы вместо записи (проверка пути чтения).
    reads: bool,
    /// Смешанный профиль: сохранения, чтения, вложения и повторные входы.
    mix: bool,
    /// Не регистрировать: входить в уже существующие аккаунты (для очень больших C).
    login_only: bool,
}

fn parse_args() -> Options {
    let mut options = Options {
        url: "https://127.0.0.1:8443".to_string(),
        users: 50,
        pushes: 10,
        concurrency: 8,
        prefix: "load".to_string(),
        duration: None,
        reads: false,
        mix: false,
        login_only: false,
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
            "--duration" => options.duration = Some(value().parse().expect("секунды")),
            "--reads" => options.reads = true,
            "--mix" => options.mix = true,
            "--login-only" => options.login_only = true,
            other => panic!("неизвестный флаг {other}"),
        }
    }
    options
}

struct User {
    login: String,
    token: String,
    identity: Arc<pms_proto::identity::Identity>,
    account: AccountId,
    device: DeviceId,
}

async fn register(
    http: &reqwest::Client,
    base: &str,
    login: &str,
    device: DeviceId,
    skip_register: bool,
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

    if skip_register {
        let token = login_user(http, base, login, &identity, account, device).await?;
        return Ok(User {
            login: login.to_string(),
            token,
            identity,
            account,
            device,
        });
    }

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
        identity: Arc::clone(&identity),
        account,
        device,
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
                match register(&http, &base, &login, device, options.login_only).await {
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
        let deadline = options
            .duration
            .map(|secs| Instant::now() + Duration::from_secs(secs));
        tasks.push(tokio::spawn(async move {
            loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                if index >= users.len() {
                    break;
                }
                let user = &users[index];
                if options.mix {
                    let mut versions: std::collections::HashMap<usize, u64> =
                        std::collections::HashMap::new();
                    let mut live_blobs: std::collections::HashMap<usize, Vec<String>> =
                        std::collections::HashMap::new();
                    // Повторный вход выдаёт новый токен и вытесняет старые сессии,
                    // поэтому внутри воркера держим актуальный.
                    let mut token = user.token.clone();

                    // Стартовая версия сейфа: нужна для If-Match, если аккаунт уже
                    // существовал до прогона (--login-only).
                    if let Ok(response) = http
                        .get(format!("{base}/v1/vault/meta"))
                        .header("x-pms-protocol", "1")
                        .bearer_auth(&token)
                        .send()
                        .await
                    {
                        let from_header = response
                            .headers()
                            .get("x-pms-vault-version")
                            .and_then(|value| value.to_str().ok())
                            .and_then(|value| value.parse::<u64>().ok());
                        let from_body = response
                            .text()
                            .await
                            .ok()
                            .and_then(|text| {
                                serde_json::from_str::<pms_proto::dto::VaultMeta>(&text).ok()
                            })
                            .map(|meta| meta.version);
                        if let Some(version) = from_header.or(from_body) {
                            versions.insert(index, version);
                        }
                    }
                    loop {
                        if let Some(deadline) = deadline {
                            if Instant::now() >= deadline {
                                break;
                            }
                        }

                        let roll: f64 = rand::random();
                        let started = Instant::now();
                        let mut ok = false;

                        if roll < 0.55 {
                            let version = *versions.entry(index).or_insert(1);
                            let body = format!("{}|v{}|{}", user.login, version, "x".repeat(2048));
                            match http
                                .put(format!("{base}/v1/vault"))
                                .header("x-pms-protocol", "1")
                                .bearer_auth(&token)
                                .header("if-match", version.to_string())
                                .header("content-length", body.len().to_string())
                                .body(body)
                                .send()
                                .await
                            {
                                Ok(response) if response.status().is_success() => {
                                    ok = true;
                                    *versions.entry(index).or_insert(1) += 1;
                                }
                                Ok(response) => {
                                    eprintln!("{}: PUT {}", user.login, response.status())
                                }
                                Err(error) => eprintln!("{}: {error}", user.login),
                            }
                        } else if roll < 0.73 {
                            match http
                                .get(format!("{base}/v1/vault"))
                                .header("x-pms-protocol", "1")
                                .bearer_auth(&token)
                                .send()
                                .await
                            {
                                Ok(response) if response.status().is_success() => {
                                    ok = response.bytes().await.is_ok();
                                }
                                Ok(response) => {
                                    eprintln!("{}: GET vault {}", user.login, response.status())
                                }
                                Err(error) => eprintln!("{}: {error}", user.login),
                            }
                        } else if roll < 0.88 {
                            // вложение 16 КиБ, как типичный скан/скриншот
                            let blob_id = format!(
                                "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
                                rand::random::<u32>(),
                                rand::random::<u16>(),
                                rand::random::<u16>(),
                                rand::random::<u16>(),
                                rand::random::<u64>() & 0xffff_ffff_ffff
                            );
                            let bytes = vec![0x5Au8; 16 * 1024];
                            let digest = blake3::hash(&bytes).to_hex().to_string();
                            match http
                                .put(format!("{base}/v1/blobs/{blob_id}"))
                                .header("x-pms-protocol", "1")
                                .bearer_auth(&token)
                                .header("x-pms-digest", digest)
                                .header("content-length", bytes.len().to_string())
                                .body(bytes)
                                .send()
                                .await
                            {
                                Ok(response) if response.status().is_success() => {
                                    ok = true;
                                    live_blobs.entry(index).or_default().push(blob_id);
                                }
                                Ok(response) => {
                                    eprintln!("{}: PUT blob {}", user.login, response.status())
                                }
                                Err(error) => eprintln!("{}: {error}", user.login),
                            }
                        } else if roll < 0.95 {
                            // удалить ранее загруженное вложение (или список, если пусто)
                            let blob_id = live_blobs.get_mut(&index).and_then(|ids| ids.pop());
                            match blob_id {
                                Some(blob_id) => match http
                                    .delete(format!("{base}/v1/blobs/{blob_id}"))
                                    .header("x-pms-protocol", "1")
                                    .bearer_auth(&token)
                                    .send()
                                    .await
                                {
                                    Ok(response) if response.status().is_success() => ok = true,
                                    Ok(response) => {
                                        eprintln!(
                                            "{}: DELETE blob {}",
                                            user.login,
                                            response.status()
                                        )
                                    }
                                    Err(error) => eprintln!("{}: {error}", user.login),
                                },
                                None => match http
                                    .get(format!("{base}/v1/blobs"))
                                    .header("x-pms-protocol", "1")
                                    .bearer_auth(&token)
                                    .send()
                                    .await
                                {
                                    Ok(response) if response.status().is_success() => ok = true,
                                    Ok(response) => {
                                        eprintln!(
                                            "{}: list blobs {}",
                                            user.login,
                                            response.status()
                                        )
                                    }
                                    Err(error) => eprintln!("{}: {error}", user.login),
                                },
                            }
                        } else {
                            // повторный вход: challenge + verify (Ed25519)
                            match login_user(
                                &http,
                                &base,
                                &user.login,
                                &user.identity,
                                user.account,
                                user.device,
                            )
                            .await
                            {
                                Ok(new_token) => {
                                    token = new_token;
                                    ok = true;
                                }
                                Err(error) => eprintln!("{}: login {error}", user.login),
                            }
                        }

                        if ok {
                            latencies
                                .lock()
                                .expect("latencies")
                                .push(started.elapsed().as_millis());
                        } else {
                            push_errors.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    continue;
                }

                if options.reads {
                    loop {
                        if let Some(deadline) = deadline {
                            if Instant::now() >= deadline {
                                break;
                            }
                        }
                        let started = Instant::now();
                        let response = http
                            .get(format!("{base}/v1/vault"))
                            .header("x-pms-protocol", "1")
                            .bearer_auth(&user.token)
                            .send()
                            .await;
                        match response {
                            Ok(response) if response.status().is_success() => {
                                let _ = response.bytes().await;
                                latencies
                                    .lock()
                                    .expect("latencies")
                                    .push(started.elapsed().as_millis());
                            }
                            Ok(response) => {
                                push_errors.fetch_add(1, Ordering::Relaxed);
                                eprintln!("{}: GET {}", user.login, response.status());
                            }
                            Err(error) => {
                                push_errors.fetch_add(1, Ordering::Relaxed);
                                eprintln!("{}: {error}", user.login);
                            }
                        }
                    }
                    continue;
                }

                let mut round: usize = 0;
                loop {
                    if let Some(deadline) = deadline {
                        if Instant::now() >= deadline {
                            break;
                        }
                    } else if round >= pushes {
                        break;
                    }
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

                    round += 1;
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
