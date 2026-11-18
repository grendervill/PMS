# PMS Server — Implementation Plan 1: workspace, протокол, сервер

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Дать работающий `pms-server` с аутентификацией по мастер-ключу, потоковым хранением сейфа и блобов, квотами и TLS, плюс общий crate `pms-proto`, на который потом сядет приложение.

**Architecture:** Cargo-workspace из трёх членов: приложение `src-tauri` (не меняется в этом плане), общий `crates/pms-proto` (identity-ключи, challenge-подпись, DTO, коды ошибок) и `crates/pms-server` (axum + rustls, SQLite только для индекса, байты — файлами на диске). Сервер не разбирает содержимое сейфа: он принимает поток байтов, считает BLAKE3, пишет атомарно и отдаёт обратно тем же потоком.

**Tech Stack:** Rust 1.90, tokio, axum 0.8, axum-server 0.7 (rustls 0.23 + aws-lc-rs), rusqlite (bundled), blake3, ed25519-dalek 2, argon2, hkdf, sha2, subtle, zeroize, mimalloc, tracing, toml, clap.

**Spec:** `docs/superpowers/specs/2026-11-18-server-sync-design.md`

## Global Constraints

- Протокол: `PROTOCOL_VERSION = 1`; версия передаётся заголовком `X-PMS-Protocol`, несовпадение — `426`.
- Тела запросов и ответов с байтами — `application/octet-stream`, без base64 и без `Content-Encoding`.
- Ошибки — JSON `{code, message}`, `code` в snake_case, тот же набор имён, что у `ApiError` в приложении.
- `account_id` — 16 байт → 32 hex-символа в нижнем регистре; пути на диске — `<2 hex>/<32 hex>`.
- `identity_params` (константа протокола, не настройка): `argon2id` `m = 131072` КиБ, `t = 4`, `p = 4`; соль — `HKDF-SHA256(canonical_secret, "PMS-IDENTITY-SALT-v1")[0..32]`, `AUTH`-семя — `HKDF-SHA256(IK, "PMS-AUTH-v1", info="ed25519-seed")[0..32]`.
- Подпись challenge покрывает `b"PMS-AUTH-v1" || account_id || device_id || nonce`.
- `tls.required = true` по умолчанию; без пригодного сертификата сервер **не стартует**.
- Квота по умолчанию — `1_073_741_824` байт; проверяется по `Content-Length` **до** чтения тела.
- `PUT /v1/vault` без `If-Match` → `428`; несовпадение версии → `409` + актуальный `VaultMeta`.
- Директории `0700`, файлы `0600`; `#![forbid(unsafe_code)]` в обоих crate'ах.
- В логах нет токенов, подписей, содержимого и `nonce`.

## Review Focus

1. Регистрация с `account_id`, не равным `BLAKE3(public_key)`, должна отвергаться, иначе чужой аккаунт можно занять только зная мастер-ключ жертвы — но проверка держит инвариант.
2. Два одновременных `PUT /v1/vault` с одним `If-Match` — ровно один `200`, второй `409`, файл не повреждён.
3. `PUT` с телом больше остатка квоты не должен прочитать тело и не должен оставить временный файл.
4. Обрыв соединения посреди `PUT` не должен оставить в каталоге ни `vault.bin`, ни мусорного `.tmp`.
5. Параллельные загрузки блобов в один аккаунт не должны позволить превысить квоту за счёт гонки счётчика.
6. Неверная подпись, полностью нулевой `nonce`, повторное использование `nonce` и истёкший токен должны давать `401`, а не панику и не `500`.

---

## Roadmap (планы 2–5, пишутся после этого)

- **План 2 — клиентский транспорт:** `core/remote.rs`, identity, pinning, команды `remote_*`, Rust-тесты против настоящего `pms-server`.
- **План 3 — режим хранения:** экран выбора, профили, запоминание, запуск из кэша, `VaultStorage::remote`.
- **План 4 — синхронизация:** pull/push, диф блобов, офлайн-очередь, конфликты, раздел настроек с полосой квоты.
- **План 5 — Android:** грант `app_data/remote` в `fs_scope`, версия 2 биометрического блока, жизненный цикл, матрица на устройстве.

Планы 2–5 пишутся по факту, а не сейчас: они зависят от того, что реально получится в этом.

## File Structure

```
Cargo.toml                                  # workspace root, members, [workspace.dependencies]
crates/pms-proto/Cargo.toml
crates/pms-proto/src/lib.rs                 # реэкспорты, PROTOCOL_VERSION
crates/pms-proto/src/identity.rs            # соль, IK, ключи, подпись challenge
crates/pms-proto/src/dto.rs                 # VaultMeta, Usage, BlobInfo, Challenge*, Verify*, Register*
crates/pms-proto/src/error.rs               # ErrorCode, WireError
crates/pms-server/Cargo.toml
crates/pms-server/src/main.rs                # CLI: run | check-config | quota-set
crates/pms-server/src/config.rs              # TOML + валидация + fail-closed TLS
crates/pms-server/src/tls.rs                  # загрузка сертификата, сборка axum-server
crates/pms-server/src/store/mod.rs            # Store: публичный интерфейс
crates/pms-server/src/store/paths.rs          # раскладка каталогов, атомарная запись
crates/pms-server/src/store/sqlite.rs         # схема и запросы индекса
crates/pms-server/src/auth.rs                 # nonce, сессии, токены, rate-limit
crates/pms-server/src/api/mod.rs              # роутер, лимиты, маппинг ошибок
crates/pms-server/src/api/auth.rs             # register, challenge, verify
crates/pms-server/src/api/vault.rs            # meta, get, put
crates/pms-server/src/api/blobs.rs            # list, head, get, put, delete
crates/pms-server/src/api/account.rs          # delete, usage
crates/pms-server/tests/support/mod.rs        # spawn сервера, генерация сертификата, клиент
crates/pms-server/tests/{config,auth,vault,blobs,quota}.rs
crates/pms-server/benches/allocations.rs      # dhat: аллокации на запрос
crates/pms-server/pms-server.example.toml
deploy/pms-server.service
deploy/README.md                              # runbook: установка, TLS, бэкап, проверка
```

Ответственности: `store` не знает про HTTP, `api` не знает про файлы, `auth` не знает ни про то, ни про другое — только про nonce/сессии/лимиты. `pms-proto` не знает ни о чём, кроме типов и криптографии идентичности.

---

### Task 1: Workspace и вывод identity-ключей в `pms-proto`

**Files:**
- Create: `Cargo.toml` (корень)
- Create: `crates/pms-proto/Cargo.toml`
- Create: `crates/pms-proto/src/lib.rs`
- Create: `crates/pms-proto/src/identity.rs`
- Modify: `src-tauri/Cargo.toml` (добавить `[package] workspace = ...` не нужно, но нужно убедиться, что workspace не ломает сборку приложения)

**Interfaces:**
- Produces: `pms_proto::PROTOCOL_VERSION: u32`, `pms_proto::identity::{IdentityKdf, IDENTITY_KDF_V1, identity_salt, derive_identity, Identity}`, где `Identity { secret: Zeroizing<[u8;32]>, public: [u8;32] }`.

- [ ] **Step 1: Создать workspace и пустой crate**

Корневой `Cargo.toml`: `[workspace] members = ["src-tauri", "crates/pms-proto", "crates/pms-server"]`, `resolver = "2"`, `[workspace.package] version = "0.1.0"`, `edition = "2021"`, `rust-version = "1.90"`. Профиль `[profile.release]`: `lto = "fat"`, `codegen-units = 1`, `panic = "unwind"`, `strip = "symbols"`.
`crates/pms-proto/Cargo.toml`: зависимости `argon2` (zeroize), `ed25519-dalek`, `hkdf`, `sha2`, `subtle`, `zeroize`, `hex`, `serde` (derive), `thiserror`.

- [ ] **Step 2: Написать падающий тест**

```rust
// crates/pms-proto/src/identity.rs
#[test]
fn same_secret_same_identity_and_salt_differs_from_other_secret() {
    let kdf = IdentityKdf::FAST_FOR_TESTS;
    let a = derive_identity(b"correct horse", &kdf);
    let b = derive_identity(b"correct horse", &kdf);
    let c = derive_identity(b"correct horse!", &kdf);
    assert_eq!(a.public, b.public);
    assert_ne!(a.public, c.public);
    assert_ne!(identity_salt(b"correct horse"), identity_salt(b"correct horse!"));
}
```

- [ ] **Step 3: Прогнать тест — убедиться, что не компилируется**

Run: `cargo test -p pms-proto identity`
Expected: FAIL — `derive_identity` not found.

- [ ] **Step 4: Реализовать `identity.rs`**

```rust
pub struct IdentityKdf { pub m_kib: u32, pub t: u32, pub p: u32 }
pub const IDENTITY_KDF_V1: IdentityKdf = IdentityKdf { m_kib: 131_072, t: 4, p: 4 };
impl IdentityKdf { pub const FAST_FOR_TESTS: Self = Self { m_kib: 8, t: 1, p: 1 }; }

pub fn identity_salt(canonical_secret: &[u8]) -> [u8; 32];
pub fn derive_identity(canonical_secret: &[u8], kdf: &IdentityKdf) -> Identity;
pub struct Identity { pub secret: Zeroizing<[u8; 32]>, pub public: [u8; 32] }
```
Соль — `HKDF-SHA256(ikm = canonical_secret, salt = b"", info = b"PMS-IDENTITY-SALT-v1")`, 32 байта. `IK` — `argon2id` версии `0x13` с параметрами `kdf` и этой солью, выход 32 байта. `secret` — `HKDF-SHA256(ikm = IK, salt = b"", info = b"PMS-AUTH-v1" || b"ed25519-seed")`, 32 байта. `public` — `SigningKey::from_bytes(&secret).verifying_key().to_bytes()`. Все промежуточные буферы — `Zeroizing`.

- [ ] **Step 5: Тест вектора протокола**

```rust
#[test]
fn identity_v1_is_pinned() {
    let identity = derive_identity(b"pms-test-vector", &IdentityKdf::FAST_FOR_TESTS);
    assert_eq!(hex::encode(identity.public), "<32 байта hex, посчитать один раз и вписать>");
}
```
Значение получить прогоном теста с `println!` и вписать в тест: это фиксирует, что изменение параметров KDF или порядка байтов ломает совместимость сразу, а не через полгода на чужом устройстве.

- [ ] **Step 6: Прогнать тесты и commit**

Run: `cargo test -p pms-proto`
Expected: PASS — 2 теста.

```bash
git add Cargo.toml crates/pms-proto
git commit -m "feat(proto): workspace and deterministic identity derivation"
```

---

### Task 2: Challenge-подпись, DTO и коды ошибок в `pms-proto`

**Files:**
- Create: `crates/pms-proto/src/error.rs`, `crates/pms-proto/src/dto.rs`
- Modify: `crates/pms-proto/src/lib.rs`, `crates/pms-proto/src/identity.rs`

**Interfaces:**
- Consumes: `Identity`, `IdentityKdf` из Task 1.
- Produces: `AccountId`, `DeviceId`, `challenge_message`, `sign_challenge`, `verify_challenge`; `VaultMeta { version: u64, size: u64, digest: String, updated_at: String }`, `Usage`, `BlobInfo { blob_id, size, digest, created_at, last_used_at }`, `ChallengeRequest/Response`, `VerifyRequest/Response`, `RegisterHeaders { account_id, public_key, device_id, digest }`; `ErrorCode` (enum со `as_str`), `WireError { code: ErrorCode, message: String }`.

- [ ] **Step 1: Тест на подпись и её отклонение**

```rust
#[test]
fn signature_binds_account_device_and_nonce() {
    let id = derive_identity(b"secret", &IdentityKdf::FAST_FOR_TESTS);
    let account = AccountId::from_public_key(&id.public);
    let device = DeviceId::new([7u8; 16]);
    let nonce = [9u8; 32];
    let sig = sign_challenge(&id, &account, &device, &nonce);

    assert!(verify_challenge(&id.public, &account, &device, &nonce, &sig));
    assert!(!verify_challenge(&id.public, &account, &DeviceId::new([8u8; 16]), &nonce, &sig));
    assert!(!verify_challenge(&id.public, &account, &device, &[10u8; 32], &sig));
    assert!(!verify_challenge(&id.public, &AccountId::new([3u8; 16]), &device, &nonce, &sig));
}
```

- [ ] **Step 2: Прогнать — FAIL**

Run: `cargo test -p pms-proto signature`
Expected: FAIL — типы не объявлены.

- [ ] **Step 3: Реализовать `AccountId`, `DeviceId`, `challenge_message`**

```rust
pub struct AccountId([u8; 16]);
impl AccountId {
    pub fn from_public_key(public: &[u8; 32]) -> Self;   // BLAKE3(public)[0..16]
    pub fn new(bytes: [u8; 16]) -> Self;
    pub fn as_bytes(&self) -> &[u8; 16];
}
impl std::fmt::Display for AccountId { /* hex, нижний регистр */ }
pub struct DeviceId([u8; 16]);
// challenge_message(account, device, nonce) -> [u8; 8 + 16 + 16 + 32] = b"PMS-AUTH-v1" || account || device || nonce
```
`BLAKE3` берётся из `blake3` (добавить в зависимости proto). Новых зависимостей больше не нужно.

- [ ] **Step 4: Реализовать `sign_challenge` / `verify_challenge`**

Подпись — `ed25519-dalek::SigningKey` из `identity.secret`, сообщение — `challenge_message`. Проверка — `VerifyingKey::from_bytes(public)`, при ошибке разбора ключа или подписи вернуть `false`, не паниковать.

- [ ] **Step 5: Реализовать DTO и коды ошибок**

```rust
#[derive(Serialize, Deserialize)] #[serde(rename_all = "camelCase")]
pub struct VaultMeta { pub version: u64, pub size: u64, pub digest: String, pub updated_at: String }
```
`ErrorCode` — `#[serde(rename_all = "snake_case")]`, значения: `BadRequest, Unauthorized, Forbidden, NotFound, Conflict, PreconditionFailed, PreconditionRequired, UnsupportedMediaType, UnsupportedProtocol, QuotaExceeded, PayloadTooLarge, RateLimited, VaultMissing, DigestMismatch, Internal, Unavailable`. `WireError` конвертируется в `(StatusCode, Json<WireError>)` в сервере (Task 8), в proto — только данные и `Display`.

- [ ] **Step 6: Тест round-trip DTO**

```rust
#[test]
fn vault_meta_serializes_camel_case() {
    let meta = VaultMeta { version: 7, size: 1024, digest: "ab".into(), updated_at: "2026-11-18T12:00:00Z".into() };
    let json = serde_json::to_string(&meta).unwrap();
    assert!(json.contains("\"version\":7") && json.contains("\"updatedAt\""));
}
```

- [ ] **Step 7: Прогнать всё и commit**

Run: `cargo test -p pms-proto`
Expected: PASS — 5 тестов. `cargo clippy -p pms-proto -- -D warnings` — чисто.

```bash
git add crates/pms-proto
git commit -m "feat(proto): challenge signatures, DTOs and wire error codes"
```

---

### Task 3: Конфиг сервера и запрет старта без TLS

**Files:**
- Create: `crates/pms-server/Cargo.toml`, `src/main.rs`, `src/config.rs`, `src/tls.rs`, `pms-server.example.toml`
- Create: `crates/pms-server/tests/config.rs`

**Interfaces:**
- Produces: `config::Config::load(path) -> Result<Config, ConfigError>`, `Config { server, tls, quota, registration, auth, limits }`, `Config::validate(&self) -> Result<ValidatedTls, ConfigError>`; `tls::TlsMode { Files { cert, key }, Off }`, `tls::resolve(config) -> Result<TlsMode, ConfigError>`.
- Produces: `Limits { max_body_bytes: u64, max_connections: usize, header_read_timeout_secs: u64, body_timeout_secs: u64 }` с дефолтами `1_073_741_824`, `512`, `10`, `300`.

- [ ] **Step 1: Написать тесты конфига**

```rust
#[test]
fn missing_tls_values_are_refused_by_default() { /* tls.required отсутствует → true, нет cert_path → ConfigError::TlsRequired */ }
#[test]
fn tls_can_be_disabled_explicitly() { /* tls.required = false, mode = "off" → Ok(TlsMode::Off), listen по умолчанию 127.0.0.1:8443 */ }
#[test]
fn zero_quota_is_rejected() { /* quota.default_bytes = 0 → ConfigError::BadQuota */ }
#[test]
fn unknown_field_is_an_error() { /* serde deny_unknown_fields: опечатка в ключе не должна молча игнорироваться */ }
```

- [ ] **Step 2: Прогнать — FAIL**

Run: `cargo test -p pms-server --test config`
Expected: FAIL — crate/типы отсутствуют.

- [ ] **Step 3: Реализовать `config.rs`**

Структуры с `#[serde(deny_unknown_fields)]`, `#[serde(default)]` только там, где значение имеет безопасный дефолт (`tls.required = true`, `quota.default_bytes = 1_073_741_824`, `auth.token_ttl_hours = 12`, `auth.max_sessions_per_account = 5`, `server.listen = "127.0.0.1:8443"`). `ConfigError` — `thiserror`, варианты: `Io, Parse, TlsRequired, BadQuota, BadPath, MissingCertificate, UnsafeOffMode`.

Правила валидации, которые тесты и проверяют:
- `tls.required && mode == "off"` → `TlsRequired`;
- `tls.required && (cert_path/key_path) == None` → `TlsRequired`;
- `!tls.required && listen` не на loopback → разрешено, но `tls::resolve` возвращает `Ok(TlsMode::Off)` и `main` печатает предупреждение;
- `quota.default_bytes == 0` → `BadQuota`;
- `limits.max_body_bytes == 0` или `limits.max_connections == 0` → `BadLimit`.

Пример `pms-server.example.toml` создаётся здесь же и содержит все секции: `[server]`, `[tls]`,
`[quota]`, `[registration]`, `[auth]`, `[limits]` — с комментариями, включая явное объяснение,
что снятие `tls.required` допустимо только для сервера в доверенной сети.

- [ ] **Step 4: Реализовать `tls::resolve`**

Читает `cert_path`/`key_path`, проверяет, что файлы существуют и что `rustls_pemfile` находит в них сертификатную цепочку и ключ. Ошибки — точные (`MissingCertificate("/etc/pms/server.crt")`). Никакой сети и ACME.

- [ ] **Step 5: Прогнать тесты — PASS**

Run: `cargo test -p pms-server --test config`
Expected: PASS — 4 теста.

- [ ] **Step 6: Commit**

```bash
git add crates/pms-server
git commit -m "feat(server): config with fail-closed TLS requirement"
```

---

### Task 4: Раскладка на диске, SQLite-индекс, создание аккаунта

**Files:**
- Create: `crates/pms-server/src/store/mod.rs`, `store/paths.rs`, `store/sqlite.rs`
- Create: `crates/pms-server/tests/store.rs`

**Interfaces:**
- Consumes: `AccountId` из `pms-proto`.
- Produces: `Store::open(data_dir: &Path, default_quota: u64) -> Result<Store>`; `Store::create_account(&self, account: AccountId, public: [u8; 32], quota: u64) -> Result<bool /* false = уже существует */>`; `Store::account(&self, account: AccountId) -> Result<Option<AccountRecord>>`; `AccountRecord { account_id, public_key, quota_bytes, used_bytes, vault_version, vault_digest, vault_size, updated_at }`; `paths::{account_dir, vault_path, blob_path, tmp_path}`.

- [ ] **Step 1: Тест раскладки и прав**

```rust
#[test]
fn account_dir_is_private_and_sharded() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), 1024).unwrap();
    let account = AccountId::new([0xab; 16]);
    assert!(store.create_account(account, [1u8; 32], 1024).unwrap());

    let path = paths::account_dir(dir.path(), account);
    assert!(path.ends_with("ab/abababababababababababababababab"));
    assert_eq!(mode(&path), 0o700);
}
#[test]
fn create_account_twice_reports_conflict() { /* второй вызов → Ok(false) */ }
```

- [ ] **Step 2: Прогнать — FAIL**

Run: `cargo test -p pms-server --test store`
Expected: FAIL.

- [ ] **Step 3: Реализовать `paths.rs`**

`account_dir(root, account) = root.join(hex[0..2]).join(hex)`, `vault_path = account_dir/vault.bin`, `blob_path = account_dir/blobs/<blob_id>` (blob_id проверяется: только `[0-9a-f-]`, ровно UUID-формат — иначе `BadRequest`, чтобы `../` не прошёл), `tmp_path = account_dir/vault.bin.tmp`. Создание каталогов с `0o700`, файлов — с `0o600` (через `OpenOptions::mode` на unix, `#[cfg(unix)]`).

- [ ] **Step 4: Реализовать `sqlite.rs`**

`PRAGMA journal_mode = WAL`, `synchronous = NORMAL`, `busy_timeout = 5000`. Схема:

```sql
CREATE TABLE accounts(account_id TEXT PRIMARY KEY, public_key BLOB NOT NULL,
  quota_bytes INTEGER NOT NULL, used_bytes INTEGER NOT NULL DEFAULT 0,
  vault_version INTEGER NOT NULL DEFAULT 0, vault_digest TEXT NOT NULL DEFAULT '',
  vault_size INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
  last_auth_at TEXT);
CREATE TABLE blobs(account_id TEXT NOT NULL, blob_id TEXT NOT NULL, size INTEGER NOT NULL,
  digest TEXT NOT NULL, created_at TEXT NOT NULL, last_used_at TEXT NOT NULL,
  PRIMARY KEY(account_id, blob_id));
CREATE TABLE sessions(token_hash BLOB PRIMARY KEY, account_id TEXT NOT NULL,
  device_id TEXT NOT NULL, expires_at INTEGER NOT NULL);
CREATE TABLE nonces(account_id TEXT NOT NULL, nonce BLOB NOT NULL, expires_at INTEGER NOT NULL,
  PRIMARY KEY(account_id, nonce));
```

`foreign_keys = ON`, `blobs.account_id` ссылается на `accounts.account_id` с `ON DELETE CASCADE`.

- [ ] **Step 5: Реализовать `Store::open` / `create_account` / `account`**

`open` создаёт `data_dir`, `data_dir/index.sqlite`, применяет схему, пересчитывает `used_bytes` для всех аккаунтов из файловой системы (это же чинит расхождения после сбоя). `create_account` — транзакция `INSERT ... ON CONFLICT DO NOTHING`, возвращает признак вставки. Ошибки — свой `StoreError` (`thiserror`), без паник.

- [ ] **Step 6: Тест пересчёта `used_bytes` при открытии**

```rust
#[test]
fn reopen_recounts_used_bytes_from_disk() {
    // создать аккаунт, положить файл 100 байт в blobs/ вручную, открыть Store заново
    // used_bytes == 100
}
```

- [ ] **Step 7: Прогнать и commit**

Run: `cargo test -p pms-server --test store`
Expected: PASS.

```bash
git add crates/pms-server
git commit -m "feat(server): on-disk layout, sqlite index, account creation"
```

---

### Task 5: Сейф — версии, `If-Match`, атомарная запись, блокировка на аккаунт

**Files:**
- Create: `crates/pms-server/src/store/vault.rs`
- Create: `crates/pms-server/tests/vault.rs`
- Modify: `crates/pms-server/src/store/mod.rs`

**Interfaces:**
- Consumes: `Store`, `paths` из Task 4; `VaultMeta` из `pms-proto`.
- Produces: `Store::vault_meta(&self, account) -> Result<Option<VaultMeta>>`; `Store::put_vault<R: AsyncRead + Unpin>(&self, account, expected_version: ExpectedVersion, body: R, len: u64) -> Result<VaultMeta, StoreError>`; `ExpectedVersion { Any, Version(u64) }`; `Store::open_vault(&self, account) -> Result<Option<(VaultMeta, tokio::fs::File)>>`; `StoreError::{Conflict(VaultMeta), PreconditionFailed, QuotaExceeded, DigestMismatch, Io}`.

- [ ] **Step 1: Написать тесты семантики версий**

```rust
#[tokio::test]
async fn first_put_is_version_one_and_second_needs_matching_version() {
    // put с ExpectedVersion::Any(при регистрации) → version 1
    // put с ExpectedVersion::Version(0) → Conflict(meta.version == 1)
    // put с ExpectedVersion::Version(1) → version 2
}
#[tokio::test]
async fn concurrent_puts_with_same_version_allow_exactly_one_winner() {
    // 8 задач ставят Version(1) с разными телами; ровно один Ok, семь Conflict
}
#[tokio::test]
async fn digest_is_blake3_of_written_bytes() { /* сравнить с blake3::hash(body) */ }
```

- [ ] **Step 2: Прогнать — FAIL**

Run: `cargo test -p pms-server --test vault`
Expected: FAIL.

- [ ] **Step 3: Реализовать блокировку на аккаунт**

В `Store`: `locks: tokio::sync::Mutex<HashMap<AccountId, Arc<tokio::sync::Mutex<()>>>>` и метод `fn account_lock(&self, account) -> Arc<tokio::sync::Mutex<()>>`, который держит map-мьютекс только на время выдачи `Arc`. Все операции записи берут этот `Arc` на весь срок операции; чтения — не берут.

- [ ] **Step 4: Реализовать `put_vault`**

Порядок, который и проверяют тесты: взять lock → прочитать `AccountRecord` → сверить `expected_version` с `vault_version` (несовпадение → `Conflict(current_meta)`, при `expected_version` больше текущей → `PreconditionFailed`) → проверить квоту `used_bytes - vault_size + len <= quota_bytes` (иначе `QuotaExceeded`) → создать `tmp_path` с `0o600` → читать `body` кусками по 64 КиБ, писать и одновременно кормить `blake3::Hasher` → `flush` + `sync_all` → `rename(tmp, vault_path)` (`tokio::fs::rename` в том же каталоге) → транзакция: обновить `vault_version + 1`, `vault_digest`, `vault_size`, `used_bytes`, `updated_at` → вернуть новый `VaultMeta`. При любой ошибке — удалить `tmp_path` (гарантировать это структурой `TempFile` с `Drop`).

- [ ] **Step 5: Тест обрыва посреди загрузки**

```rust
#[tokio::test]
async fn aborted_upload_leaves_no_vault_and_no_tmp() {
    // body-ридер, отдающий 1000 байт и возвращающий io::Error
    // put_vault → Err; каталог не содержит ни vault.bin, ни *.tmp; meta == None
}
```

- [ ] **Step 6: Реализовать `open_vault` / `vault_meta`**

`open_vault` открывает файл и возвращает вместе с метой; не читает содержимое в память.

- [ ] **Step 7: Прогнать и commit**

Run: `cargo test -p pms-server --test vault`
Expected: PASS — 5 тестов.

```bash
git add crates/pms-server
git commit -m "feat(server): vault versions with If-Match, atomic writes, per-account lock"
```

---

### Task 6: Блобы, квота и `usage`

**Files:**
- Create: `crates/pms-server/src/store/blobs.rs`
- Create: `crates/pms-server/tests/blobs.rs`, `tests/quota.rs`
- Modify: `crates/pms-server/src/store/mod.rs`

**Interfaces:**
- Consumes: `Store`, `paths` из Task 4, `BlobInfo`/`Usage` из `pms-proto`.
- Produces: `Store::put_blob<R: AsyncRead + Unpin>(&self, account, blob_id: &str, digest: &str, body: R, len: u64) -> Result<BlobInfo, StoreError>`; `Store::get_blob(&self, account, blob_id) -> Result<Option<(BlobInfo, tokio::fs::File)>>`; `Store::blob_info(&self, account, blob_id)`; `Store::list_blobs(&self, account) -> Result<Vec<BlobInfo>>`; `Store::delete_blob(&self, account, blob_id) -> Result<bool>`; `Store::usage(&self, account) -> Result<Usage>`; `Store::bump_blob_use(&self, account, blob_id)`.

- [ ] **Step 1: Тесты идемпотентности и квоты**

```rust
#[tokio::test]
async fn same_blob_twice_is_noop_and_keeps_size() { /* put, put → used_bytes не удвоился, BlobInfo тот же */ }
#[tokio::test]
async fn digest_mismatch_removes_temp_file() { /* ждать DigestMismatch, каталог чист */ }
#[tokio::test]
async fn quota_counts_vault_and_blobs_together() {
    // quota = 1000; vault 400; blob 500 ok; второй blob 200 → QuotaExceeded
}
#[tokio::test]
async fn parallel_blob_uploads_cannot_exceed_quota() {
    // quota = 1000, 8 задач по 300 байт одновременно → 3 успеха, 5 QuotaExceeded, used ≤ 900
}
```

- [ ] **Step 2: Прогнать — FAIL**

Run: `cargo test -p pms-server --test blobs --test quota`
Expected: FAIL.

- [ ] **Step 3: Реализовать `put_blob`**

Тот же приём, что `put_vault`: lock аккаунта → существующий блоб с тем же `digest` возвращается как есть (`last_used_at` обновляется) → проверка квоты → поток в `tmp_path` с `blake3::Hasher` → сверить с заявленным `digest` (несовпадение → `DigestMismatch`, temp удаляется) → `rename` → транзакция `INSERT ... ON CONFLICT DO UPDATE`, `used_bytes += len`. `blob_id` валидируется как UUID (32 hex-символа с дефисами или без).

- [ ] **Step 4: Реализовать `usage`, `list_blobs`, `delete_blob`, `bump_blob_use`**

`usage` считает `vault_bytes` из записи аккаунта, `blobs_bytes` и `blob_count` из таблицы `blobs`, `used = сумма`. `delete_blob` — lock, удаление файла, затем строка и `used_bytes -= size`. `bump_blob_use` обновляет `last_used_at` (вызывается на `GET`/`HEAD`, без блокировки аккаунта —UPDATE по первичному ключу).

- [ ] **Step 5: Прогнать и commit**

Run: `cargo test -p pms-server --test blobs --test quota`
Expected: PASS — 4 теста.

```bash
git add crates/pms-server
git commit -m "feat(server): blob storage with idempotency, digest check and quota"
```

---

### Task 7: Аутентификация — nonce, сессии, токены, rate-limit

**Files:**
- Create: `crates/pms-server/src/auth.rs`
- Create: `crates/pms-server/tests/auth.rs`
- Modify: `crates/pms-server/src/store/mod.rs` (методы nonce/session)

**Interfaces:**
- Consumes: `AccountId`, `DeviceId` из `pms-proto`, `Store` из Task 4.
- Produces: `Auth::new(store: Arc<Store>, limits: Limits) -> Auth`; `Auth::issue_challenge(&self, account, device) -> Result<Challenge>`; `Auth::verify(&self, account, device, nonce: &[u8;32], signature: &[u8;64], public_key: &[u8;32]) -> Result<SessionToken>`; `Auth::authenticate(&self, token: &str) -> Result<Authenticated { account, device }>`; `Auth::revoke_all(&self, account)`; `Auth::rate_limiter(&self) -> &RateLimiter`; `RateLimiter::check(&self, key: &str) -> Result<(), NotBefore>`.

- [ ] **Step 1: Тесты**

```rust
#[tokio::test]
async fn nonce_is_single_use() { /* второй verify с тем же nonce → Unauthorized */ }
#[tokio::test]
async fn zero_nonce_and_zero_signature_are_rejected() { /* nonce [0;32] → BadRequest; sig [0;64] → Unauthorized */ }
#[tokio::test]
async fn expired_nonce_is_rejected() { /* TTL 60 с; вставить nonce с истёкшим expires_at → Unauthorized */ }
#[tokio::test]
async fn token_lookup_is_exact_and_revocable() { /* authenticate ok; revoke_all → Unauthorized */ }
#[tokio::test]
async fn rate_limiter_backs_off_after_five_failures() { /* 5 неудач → NotBefore, 6-я попытка тоже отказ */ }
```

- [ ] **Step 2: Прогнать — FAIL** · Run: `cargo test -p pms-server --test auth`

- [ ] **Step 3: Реализовать `Auth`**

Nonce — 32 случайных байта из `rand::rngs::OsRng`, TTL из конфига (`auth.challenge_ttl_seconds`, по умолчанию 60). Хранение — таблица `nonces`; при `verify` nonce удаляется до проверки подписи (одноразовость не зависит от исхода). Токен — 32 случайных байта, клиенту отдаётся hex, в базе — `SHA-256` от hex-строки (32 байта), поиск по первичному ключу, сравнение — `subtle::ConstantTimeEq`. Сессии ограничены `auth.max_sessions_per_account` (при превышении удаляется самая старая по `expires_at`), TTL — `auth.token_ttl_hours`.

При неудаче подписи — инкремент счётчика `RateLimiter` по ключу `account_id`; порог 5 в минуту, затем отказ до `NotBefore` с экспоненциальным удвоением (5, 10, 20, 40, 60 секунд максимум). Ключ не должен содержать `signature`/`nonce`.

- [ ] **Step 4: Тест «чужой ключ не проходит»**

```rust
#[tokio::test]
async fn attacker_key_cannot_authenticate_account() {
    // challenge для account A, подпись ключом B → Unauthorized, сессия не создана
}
```

- [ ] **Step 5: Прогнать и commit**

Run: `cargo test -p pms-server --test auth`
Expected: PASS — 6 тестов.

```bash
git add crates/pms-server
git commit -m "feat(server): single-use nonce auth, sessions and rate limiting"
```

---

### Task 8: HTTP-слой — роутер, лимиты, маппинг ошибок, регистрация, `healthz`

**Files:**
- Create: `crates/pms-server/src/api/mod.rs`, `src/api/auth.rs`
- Create: `crates/pms-server/tests/support/mod.rs`, `tests/http.rs`
- Modify: `crates/pms-server/src/main.rs`

**Interfaces:**
- Consumes: `Config` (Task 3), `Store` (Task 4), `Auth` (Task 7), DTO из `pms-proto`.
- Лимиты (`max_body_bytes`, `max_connections`, таймауты) берутся из секции `config.limits`, объявленной в Task 3.
- Produces: `api::router(state: AppState) -> axum::Router`; `AppState { store: Arc<Store>, auth: Arc<Auth>, config: Arc<Config> }`; `TestServer::spawn_with(config_overrides) -> TestServer` c полями `base_url`, `client: reqwest::Client` (в тестах).

- [ ] **Step 1: Тест «общие» **

```rust
#[tokio::test]
async fn healthz_returns_ok_without_auth() { /* GET /v1/healthz → 200, тело {"status":"ok"} */ }
#[tokio::test]
async fn unknown_route_is_json_404() { /* GET /v1/nope → 404 + {"code":"not_found"} */ }
#[tokio::test]
async fn wrong_protocol_header_is_426() { /* X-PMS-Protocol: 2 → 426 unsupported_protocol */ }
#[tokio::test]
async fn oversized_header_block_is_rejected() { /* заголовок 64 КиБ → 431 или 400, не паника */ }
```

- [ ] **Step 2: Прогнать — FAIL** · Run: `cargo test -p pms-server --test http`

- [ ] **Step 3: Реализовать `api/mod.rs`**

`Router` с `DefaultBodyLimit::max(config.limits.max_body_bytes)`; middleware: `TraceLayer` без тел, проверка `X-PMS-Protocol`, `ConcurrencyLimitLayer` на `config.limits.max_connections`. Единый тип `ApiError(WireError)` с `IntoResponse` → `(StatusCode, Json<WireError>)`. Маппинг: `StoreError::Conflict(meta)` → `409` + JSON `{code:"conflict", message, ...}` и заголовки `X-PMS-Vault-Version`, `X-PMS-Vault-Digest`; `QuotaExceeded` → `413 quota_exceeded`; `DigestMismatch` → `400 digest_mismatch`; `NotFound` → `404`; всё прочее → `500 internal` без деталей наружу (детали — в `tracing::error!`).

- [ ] **Step 4: Тест маппинга**

```rust
#[tokio::test]
async fn error_codes_are_stable_json() {
    // PUT /v1/vault без авторизации → 401 {"code":"unauthorized","message":...}
    // PUT с неизвестным Content-Length меньше тела → 400 bad_request
}
```

- [ ] **Step 5: Реализовать `POST /v1/accounts`**

Заголовки: `X-PMS-Account-Id` (32 hex), `X-PMS-Public-Key` (64 hex), `X-PMS-Device-Id` (32 hex), `X-PMS-Signature` (128 hex), тело — байты сейфа (может быть пустым). Проверки по порядку: формат заголовков → `AccountId::from_public_key(pk) == account_id` (иначе `400 bad_request`) → подпись над `b"PMS-REGISTER-v1" || account || device || BLAKE3(body)` (иначе `401`) → `registration.token` при непустом конфиге (`403 forbidden`) → `store.create_account` (`409` если занят) → если тело не пусто, `put_vault` с `ExpectedVersion::Any` → `201` + `VaultMeta` (или `{"version":0}` без тела).

- [ ] **Step 6: Тесты регистрации**

```rust
#[tokio::test]
async fn register_with_mismatched_account_id_is_rejected() { /* 400 */ }
#[tokio::test]
async fn register_with_bad_signature_is_rejected() { /* 401, аккаунт не создан */ }
#[tokio::test]
async fn register_then_register_again_is_409() { /* 409 conflict */ }
#[tokio::test]
async fn register_with_vault_sets_version_one() { /* 201, meta.version == 1 */ }
```

- [ ] **Step 7: Прогнать и commit**

Run: `cargo test -p pms-server --test http`
Expected: PASS — 8 тестов.

```bash
git add crates/pms-server
git commit -m "feat(server): router, limits, error mapping and registration"
```

---

### Task 9: Аутентификация и сейф по HTTP

**Files:**
- Create: `crates/pms-server/src/api/vault.rs`
- Modify: `crates/pms-server/src/api/mod.rs`, `src/api/auth.rs`
- Create: `crates/pms-server/tests/vault_http.rs`

**Interfaces:**
- Consumes: всё из Task 5, 7, 8.
- Produces: эндпоинты `POST /v1/auth/challenge`, `POST /v1/auth/verify`, `GET /v1/vault/meta`, `GET /v1/vault`, `PUT /v1/vault`; экстрактор `Authed(account, device)`.

- [ ] **Step 1: Тесты потока входа**

```rust
#[tokio::test]
async fn challenge_then_verify_yields_working_token() { /* verify → token; GET /v1/vault/meta с Bearer → 200 */ }
#[tokio::test]
async fn challenge_for_unknown_account_is_404_and_does_not_reveal_existence() {
    /* одинаковый ответ на неизвестный аккаунт и на неверную подпись? нет: 404 для неизвестного,
       401 для неверной подписи — так клиент отличает "создать аккаунт" от "неверный ключ";
       404 без тела, чтобы нельзя было отличить по размеру */
}
#[tokio::test]
async fn put_vault_without_if_match_is_428() { /* 428 precondition_required */ }
#[tokio::test]
async fn put_vault_with_stale_version_is_409_with_current_meta_headers() { /* 409 + X-PMS-Vault-Version */ }
#[tokio::test]
async fn get_vault_streams_exact_bytes_with_etag() { /* тело == загруженному, ETag: "v1-<digest>" */ }
```

- [ ] **Step 2: Прогнать — FAIL** · Run: `cargo test -p pms-server --test vault_http`

- [ ] **Step 3: Реализовать экстрактор `Authed`**

`impl FromRequestParts<AppState> for Authed`: берёт `Authorization: Bearer <hex>`, `auth.authenticate` → `Authenticated { account, device }`, иначе `401`. Не логирует токен. Проверяет `X-PMS-Protocol` ещё раз (middleware мог быть обойдён при вложенных роутерах).

- [ ] **Step 4: Реализовать эндпоинты сейфа**

`GET /v1/vault/meta` → `200 VaultMeta` или `404 vault_missing`. `GET /v1/vault` → `Content-Type: application/octet-stream`, `Content-Length`, `ETag: "v{version}-{digest}"`, тело — `tokio_util::io::ReaderStream` поверх `tokio::fs::File` (никакой буферизации целиком). `PUT /v1/vault` — `If-Match` парсится в `ExpectedVersion` (`"*"` → `Any`, число → `Version`), тело — `axum::body::Body` → `BodyDataStream` → `AsyncRead` (`tokio_util::io::StreamReader`), `Content-Length` обязателен (`411`), лимит — из `config.limits.max_body_bytes`, квота проверяется внутри `put_vault` до чтения тела.

- [ ] **Step 5: Тест «двое одновременно» через HTTP**

```rust
#[tokio::test]
async fn two_clients_with_same_version_one_gets_409() {
    // оба PUT If-Match: 1 параллельно → {200, 409}, и GET отдаёт тело победителя
}
```

- [ ] **Step 6: Прогнать и commit**

Run: `cargo test -p pms-server --test vault_http`
Expected: PASS — 6 тестов.

```bash
git add crates/pms-server
git commit -m "feat(server): auth and vault endpoints with streaming bodies"
```

---

### Task 10: Блобы, usage и удаление аккаунта по HTTP

**Files:**
- Create: `crates/pms-server/src/api/blobs.rs`, `src/api/account.rs`
- Modify: `crates/pms-server/src/api/mod.rs`
- Create: `crates/pms-server/tests/blobs_http.rs`

**Interfaces:**
- Consumes: Task 6, 7, 9.
- Produces: `GET /v1/blobs`, `HEAD /v1/blobs/{id}`, `GET /v1/blobs/{id}`, `PUT /v1/blobs/{id}`, `DELETE /v1/blobs/{id}`, `GET /v1/usage`, `DELETE /v1/account`.

- [ ] **Step 1: Тесты**

```rust
#[tokio::test]
async fn blob_put_requires_digest_header() { /* без X-PMS-Digest → 400 bad_request */ }
#[tokio::test]
async fn blob_put_with_wrong_digest_is_400_and_leaves_nothing() { /* 400 digest_mismatch */ }
#[tokio::test]
async fn head_blob_bumps_last_used_at() { /* created_at != last_used_at после HEAD */ }
#[tokio::test]
async fn usage_matches_uploaded_sizes() { /* usage.used == vault + sum(blobs) */ }
#[tokio::test]
async fn delete_account_removes_files_and_sessions() {
    /* DELETE /v1/account (с подписью) → 200; старый токен → 401; каталог пуст */
}
```

- [ ] **Step 2: Прогнать — FAIL** · Run: `cargo test -p pms-server --test blobs_http`

- [ ] **Step 3: Реализовать эндпоинты блобов**

`blob_id` проверяется регэкспом `^[0-9a-f]{8}-?[0-9a-f]{4}-?[0-9a-f]{4}-?[0-9a-f]{4}-?[0-9a-f]{12}$`; иначе `400`. `PUT` требует `X-PMS-Digest` (64 hex), `Content-Length`; тело — поток. `GET`/`HEAD` вызывают `bump_blob_use` (ошибка обновления не должна ломать выдачу файла — логируется). `DELETE` → `204` или `404`.

- [ ] **Step 4: Реализовать `DELETE /v1/account`**

Требует `X-PMS-Signature` над `b"PMS-ACCOUNT-DELETE-v1" || account || device` и валидный токен. Удаляет каталог аккаунта (`tokio::fs::remove_dir_all`), строки `accounts`/`blobs`/`sessions`/`nonces` в одной транзакции, возвращает `200 {"deleted":true}`.

- [ ] **Step 5: Прогнать и commit**

Run: `cargo test -p pms-server --test blobs_http`
Expected: PASS — 6 тестов.

```bash
git add crates/pms-server
git commit -m "feat(server): blob, usage and account-deletion endpoints"
```

---

### Task 11: TLS-листенер, CLI и end-to-end тест с самоподписанным сертификатом

**Files:**
- Create: `crates/pms-server/src/tls.rs` (расширить), `src/lib.rs`
- Modify: `crates/pms-server/src/main.rs`
- Create: `crates/pms-server/tests/tls.rs`, `tests/support/mod.rs` (расширить)

**Interfaces:**
- Consumes: `Config`, `api::router`, `AppState`.
- Produces: `pms_server::run(config: Config) -> Result<ServerHandle>`; `ServerHandle { addr: SocketAddr, shutdown: oneshot::Sender<()> , join: JoinHandle<()> }`; CLI `pms-server run --config <path>`, `pms-server check-config --config <path>`, `pms-server quota-set --config <path> --account <hex> --bytes <n>`.

- [ ] **Step 1: Тесты**

```rust
#[tokio::test]
async fn server_refuses_to_start_without_cert_when_required() { /* run → Err(TlsRequired), порт не занят */ }
#[tokio::test]
async fn off_mode_listens_on_loopback_and_serves_healthz() { /* tls.required = false → healthz 200 по http */ }
#[tokio::test]
async fn tls_mode_serves_healthz_over_self_signed_cert() {
    /* rcgen-сертификат с SAN 127.0.0.1, reqwest с add_root_certificate → 200 */
}
#[tokio::test]
async fn tls_handshake_fails_for_untrusted_cert() { /* клиент без корня → ошибка соединения, не 200 */ }
```

- [ ] **Step 2: Прогнать — FAIL** · Run: `cargo test -p pms-server --test tls`

- [ ] **Step 3: Реализовать `tls.rs` и `run`**

`run`: `config.validate()` → `Store::open` → `Auth::new` → `api::router` → в зависимости от `TlsMode`: `axum_server::bind_rustls(addr, RustlsConfig::from_pem(cert, key))` или `axum_server::bind(addr)` при `Off`. Graceful shutdown по `oneshot` + `tokio::signal::ctrl_c`. В лог при старте: `listen`, `data_dir`, `tls mode`, `quota default`, версия протокола. При `Off` — `tracing::warn!` о незашифрованном режиме.

- [ ] **Step 4: Реализовать `main.rs` с `clap`**

`run`, `check-config` (печатает разобранный конфиг без секретов и код выхода 0/1), `quota-set` (SQLite `UPDATE accounts SET quota_bytes = ?`). Никаких других подкоманд.

- [ ] **Step 5: Прогнать и commit**

Run: `cargo test -p pms-server --test tls`
Expected: PASS — 4 теста.

```bash
git add crates/pms-server
git commit -m "feat(server): TLS listener, CLI and end-to-end TLS test"
```

---

### Task 12: Сквозной сценарий, бенчмарк аллокаций и runbook

**Files:**
- Create: `crates/pms-server/tests/e2e.rs`, `benches/allocations.rs`
- Create: `deploy/pms-server.service`, `deploy/README.md`
- Modify: `README.md` (раздел про сервер)

**Interfaces:**
- Consumes: `TestServer` из `tests/support`, `pms-proto`.

- [ ] **Step 1: Сквозной тест «устройство A и устройство B»**

```rust
#[tokio::test]
async fn two_devices_sync_same_vault() {
    // A: derive_identity(secret) → register с пустым сейфом → PUT If-Match 0 → version 1
    // A: PUT blob → GET /v1/usage (used == vault + blob)
    // B: derive_identity(тот же secret) → challenge → verify → GET meta == version 1
    // B: GET vault → байты совпадают с A
    // A: PUT If-Match 1 → version 2; B: PUT If-Match 1 → 409 + актуальный meta
}
```

- [ ] **Step 2: Прогнать — PASS без правок**

Run: `cargo test -p pms-server --test e2e`
Expected: PASS. Если падает — чинить причину, а не тест.

- [ ] **Step 3: Бенчмарк аллокаций**

```rust
// benches/allocations.rs: dhat-профайлер вокруг N=100 последовательных PUT 64 КиБ блобов
// и N HEAD-запросов; печатает total_blocks / N и пиковый RSS.
#[test] fn allocations_per_blob_put_are_bounded() { /* assert!(blocks_per_request <= 200) */ }
```
Порог — измеренное значение с запасом; смысл теста в том, чтобы регрессия «начали читать тело целиком в память» падала сразу.

- [ ] **Step 4: Проверить память на большом файле**

```bash
cargo run --release -p pms-server -- check-config --config crates/pms-server/pms-server.example.toml
# затем вручную: загрузить 500 МиБ blob, замерить RSS процесса (должен остаться ниже ~64 МиБ)
```
Результат записать в `deploy/README.md`.

- [ ] **Step 5: Написать runbook**

`deploy/README.md`: установка (systemd unit, `User=pms`, `StateDirectory=pms`, `NoNewPrivileges`, `ProtectSystem=strict`, `ReadWritePaths=/var/lib/pms`), выпуск сертификата через certbot или свой CA с **SAN** (`openssl req -x509` + `-addext "subjectAltName=DNS:pms.home,IP:192.168.1.10"`), проверка `curl --cacert`, бэкап `sqlite3 index.sqlite ".backup"` + `rsync` каталога, обновление бинарника, диагностика (`journalctl -u pms-server`), лимиты файрвола.

- [ ] **Step 6: Прогнать всё**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check`
Expected: PASS, без предупреждений.

- [ ] **Step 7: Commit**

```bash
git add crates/pms-server deploy README.md
git commit -m "test(server): end-to-end sync scenario, allocation bench and runbook"
```

---

### Task 13: Развернуть на тестовом сервере и проверить руками

**Files:** (вне репозитория — на `192.168.1.233`)

- [ ] **Step 1: Собрать под целевой хост**

```bash
cargo build --release -p pms-server --target x86_64-unknown-linux-gnu
```
(При сборке на macOS — через `cross` или на самом сервере через rustup + `apt install build-essential cmake`; `cmake` нужен для `aws-lc-rs`.)

- [ ] **Step 2: Установить и запустить**

`scp` бинарник и `pms-server.example.toml` → `/etc/pms/server.toml`, выпустить сертификат с SAN для `192.168.1.233`, `systemctl enable --now pms-server`, `journalctl -u pms-server -n 20` — старт без ошибок.

- [ ] **Step 3: Прогнать сквозной сценарий curl'ом**

```bash
curl --cacert /tmp/ca.crt https://192.168.1.233:8443/v1/healthz          # 200
curl --insecure https://192.168.1.233:8443/v1/healthz                    # тоже 200 (только для проверки)
```

- [ ] **Step 4: Записать в `deploy/README.md` фактические цифры**

RSS после загрузки 500 МиБ, скорость загрузки/выгрузки на локальной сети, размер каталога данных.

- [ ] **Step 5: Commit заметок**

```bash
git add deploy/README.md
git commit -m "docs(deploy): verify server on test VM, record measured numbers"
```

---

## Self-Review

**Покрытие спеки.** §7 идентичность и вход — Task 1, 2, 7, 8. §8 хранилище — Task 4, 5, 6. §9 API и лимиты — Task 8–10. §10 клиент — планы 2–5 (в этом плане только контракт). §11 TLS — Task 3, 11, 13. §12 производительность и безопасность — Task 5, 6, 12 (плюс `forbid(unsafe_code)`, `0700/0600`, лимиты в Task 3, 4, 8). §14 тесты — все задачи. §15 этапы 1–3, 9 — этот план.

**Границы.** Квоты на аккаунт сверх дефолта меняются только CLI (`quota-set`), админки нет — это осознанно и покрыто Task 11.

**Отложено сознательно.** `dhat`-бенчмарк в Task 12 — порог грубый; точная настройка после появления реального профиля трафика. Фаззинг разбора заголовков отнесён к плану 4 (этап «закалка»), чтобы не раздувать первый план.
