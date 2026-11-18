# PMS Server — Implementation Plan 2: клиентский транспорт

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Приложение умеет говорить с `pms-server`: регистрироваться, входить по мастер-ключу, тянуть и отправлять сейф с `If-Match`, работать с блобами и квотой — с проверкой TLS и понятными ошибками, без единой строчки UI.

**Architecture:** Новый слой `src-tauri/src/core/remote/` поверх `reqwest` + `rustls`, плюс `commands/remote.rs`, который держит токены и профили в памяти Rust. Сети в рендерере нет: webview получает только результат операций. Транспорт тестируется против настоящего `pms-server`, поднятого в том же тесте на loopback с самоподписанным сертификатом.

**Tech Stack:** Rust, reqwest 0.12 (rustls 0.23, ring), x509-parser для SPKI-pin, pms-proto, tokio, rcgen (тесты), pms-server как dev-dependency.

**Spec:** `docs/superpowers/specs/2026-11-18-server-sync-design.md` (§10 клиент, §11 TLS)
**Предыдущий план:** `docs/superpowers/plans/2026-11-18-server-sync-01-server.md`

## Global Constraints

- Сеть только из Rust. В рендерер не уходят ни токены, ни `auth_seed`, ни байты сейфа сверх существующих путей.
- `http://` запрещён, если профиль явно не `Insecure`; проверка — до первого запроса.
- `TlsPolicy`: `System` (корни), `Pin { spki_sha256 }` (отпечаток SPKI, hex в нижнем регистре), `Insecure` (только для локального сервера, UI обязан предупреждать).
- Ошибки транспорта мапятся в `ApiError` с кодами `remote_unreachable`, `remote_tls`, `remote_insecure_forbidden`, `remote_protocol`, `remote_conflict`, `remote_quota_exceeded`, `remote_unauthorized`, `remote_not_found`, `remote_server`, `remote_bad_request`.
- Конфликт версий — не исключение: `RemoteError::Conflict(VaultMeta)`.
- Device id — 16 случайных байт в `app_data/remote-device.json`, стабилен между запусками.
- Сейф и блобы качаются/заливаются потоком через файл, не через `Vec<u8>` в памяти.
- `#![forbid(unsafe_code)]` остаётся; новых `unsafe` нет.

## Review Focus

1. Профиль с `http://` и `TlsPolicy::System` не должен отправить ни одного байта — ошибка до запроса.
2. `Pin` с чужим отпечатком должен рвать соединение, а не «принять и продолжить».
3. `push` со старой версией должен вернуть `Conflict` с метаданными сервера, а не общую ошибку.
4. Обрыв соединения посреди `pull` не должен оставить обрезанный `pms.vault` по целевому пути.
5. Повторный `login` тем же профилем не должен плодить сессии в памяти (или должен ограничивать их число).
6. Регистрация уже существующего аккаунта должна приводить к понятному «войдите», а не к падению.

---

### Task 1: Профиль, политика TLS и пиннинг сертификата

**Files:**
- Create: `src-tauri/src/core/remote/mod.rs`, `src-tauri/src/core/remote/tls.rs`
- Modify: `src-tauri/src/core/mod.rs`, `src-tauri/Cargo.toml` (`x509-parser`, dev: `rcgen`, `pms-server`, `tokio`)
- Test: `src-tauri/src/core/remote/tls.rs` (юнит), `src-tauri/tests/remote_tls.rs`

**Interfaces:**
- Produces: `RemoteProfile { base_url: String, tls: TlsPolicy }`; `TlsPolicy { System, Pin { spki_sha256: String }, Insecure }`; `fn build_client(profile: &RemoteProfile) -> Result<reqwest::Client, RemoteError>`; `fn parse_base_url(profile: &RemoteProfile) -> Result<url::Url, RemoteError>`; `async fn fetch_spki_sha256(base_url: &str) -> Result<String, RemoteError>` (для UI при первом подключении); `RemoteError` с `code()`.

- [ ] **Step 1: Тесты политики (без сети)**

```rust
#[test] fn http_is_refused_unless_insecure()
#[test] fn https_with_insecure_policy_is_allowed()
#[test] fn pin_accepts_lowercase_hex_only()
```
- [ ] **Step 2: Прогнать — FAIL** · `cargo test -p pms --lib remote` → нет модуля.
- [ ] **Step 3: Реализовать `parse_base_url` + `build_client`**
- [ ] **Step 4: Тест пиннинга против настоящего сервера** (`tests/remote_tls.rs`): сгенерировать `rcgen`-сертификат с SAN `127.0.0.1`, поднять `pms_server::run`, проверить: свой отпечаток — `Ok`, чужой — `Err(Tls)`, `System` — `Err(Tls)`, `Insecure` — `Ok`.
- [ ] **Step 5: Прогнать — PASS**, commit.

### Task 2: Device id, probe, регистрация и вход

**Files:**
- Create: `src-tauri/src/core/remote/client.rs`, `src-tauri/src/core/remote/identity.rs`
- Create: `src-tauri/tests/remote_auth.rs`
- Modify: `src-tauri/src/core/remote/mod.rs`

**Interfaces:**
- Produces: `RemoteClient { profile, http, base: Url }`; `RemoteClient::new(profile)`; `probe() -> Result<Probe { protocol, server }>`; `identity_from_secret(&UnlockSecretInput) -> Result<(Identity, AccountId), RemoteError>`; `device_id() -> DeviceId` (файл `remote-device.json`); `register(&self, identity, vault: &[u8]) -> Result<VaultMeta, RemoteError>`; `login(&self, identity) -> Result<Login { token, expires_at, account }, RemoteError>`; `SessionToken` (zeroize, не печатается).

- [ ] **Step 1: Тесты**: device id стабилен между вызовами и файлом; `probe` возвращает `protocol == 1`; повторный `register` → `RemoteError::Conflict`; `login` даёт токен, который проходит `vault_meta`; неверный ключ → `Unauthorized`.
- [ ] **Step 2: FAIL** → реализовать → **PASS** → commit.

### Task 3: Сейф — метаданные, скачивание и отправка потоком

**Interfaces:**
- Produces: `vault_meta(&self, token) -> Result<Option<VaultMeta>, RemoteError>`; `pull_vault(&self, token, dest_dir: &Path) -> Result<VaultMeta, RemoteError>` (пишет `dest_dir/pms.vault` через `.tmp` + rename); `push_vault(&self, token, expected: Option<u64>, file: &Path) -> Result<VaultMeta, RemoteError>` (поток из файла, `If-Match`).

- [ ] **Step 1: Тесты**: `pull` кладёт байты ровно как на сервере; повторный `push` со старой версией → `Conflict` + версия сервера; `push` без версии (`None`) → `If-Match: *`; обрыв (сервер убит) посреди `pull` не оставляет `pms.vault`.
- [ ] **Step 2: FAIL** → реализовать → **PASS** → commit.

### Task 4: Блобы, квота, удаление аккаунта

**Interfaces:**
- Produces: `list_blobs(&self, token)`; `put_blob(&self, token, blob_id, file: &Path)`; `get_blob(&self, token, blob_id, dest: &Path)`; `delete_blob(&self, token, blob_id)`; `usage(&self, token) -> Usage`; `delete_account(&self, token, identity)`.

- [ ] **Step 1: Тесты**: загрузка/список/скачивание/удаление; `usage` растёт и падает; квота: сервер с `quota.default_bytes` маленькой → `QuotaExceeded`; удаление аккаунта инвалидирует токен.
- [ ] **Step 2: FAIL** → реализовать → **PASS** → commit.

### Task 5: Команды `remote_*` и реестр сессий

**Files:**
- Create: `src-tauri/src/commands/remote.rs`
- Modify: `src-tauri/src/commands/mod.rs` (реестр в `AppState`), `src-tauri/src/lib.rs` (оба списка команд)
- Create: `src-tauri/tests/remote_commands.rs`

**Interfaces:**
- Produces: `RemoteRegistry { store(account, RemoteSession), token(account), remove(account), list() }`, `RemoteSession { profile, token, account, device, auth_seed: Zeroizing<[u8;32]>, expires_at }`; команды `remote_probe`, `remote_fingerprint`, `remote_register`, `remote_login`, `remote_logout`, `remote_status`, `remote_vault_meta`, `remote_pull_vault`, `remote_push_vault`, `remote_list_blobs`, `remote_upload_blob`, `remote_download_blob`, `remote_delete_blob`, `remote_usage`, `remote_delete_account`.

- [ ] **Step 1: Тесты**: команды отвечают на `MockRuntime` без AppHandle; `remote_probe` к выключенному порту → `remote_unreachable`; `remote_login` кладёт сессию, `remote_status` её видит, `remote_logout` убирает; `remote_push_vault` без сессии → `remote_unauthorized`.
- [ ] **Step 2: FAIL** → реализовать → **PASS** → commit.

### Task 6: Документация и проверка на живом сервере

- [ ] **Step 1: `docs/reports/2026-11-19-remote-client.md`** — что сделано, какие коды ошибок, результат тестов.
- [ ] **Step 2: Прогон против VM** (`192.168.1.233`): тест-бинарь или игнорируемый тест `remote_live_vm` под `#[ignore]`, запускается вручную с `PMS_TEST_SERVER`/`PMS_TEST_PIN`.
- [ ] **Step 3: `cargo test --workspace`, `clippy -D warnings`, `fmt --check`** → commit.

## Self-Review

Клиентский кэш, экран выбора режима, движок синхронизации и настройки квоты — план 3 и 4. Android-грант и биометрический блок версии 2 — план 5. Список блобов и `use` здесь только транспорт: решения «что качать» принимает движок синхронизации.
