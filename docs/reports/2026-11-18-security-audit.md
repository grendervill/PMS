# Аудит безопасности PMS (v0.3.2)

Дата: 18.11.2026 · Ветка: `main` (`388d641`) · Объём: Rust-ядро 10 281 строка (36 файлов), фронтенд 19 513 строк, конфиг Tauri 2.12, Android-манифест.

Документ — результат ручного разбора кода, а не прогон готовых тестов. Ниже: методика, модель угроз, находки с привязкой к строкам, план устранения (P0→P2) с критериями приёмки.

---

## 1. Как проверялось

| Шаг | Команда / приём | Результат |
|---|---|---|
| Разбор ядра | чтение `core/{crypto,vault,vault_file,session,models,attachments,totp,ssh,share,generator,import*,pms_archive}.rs`, `commands/*.rs`, `lib.rs` целиком | см. находки |
| Разбор фронта | `src/lib/*`, `src/features/vault/*`, `attachments/*`, `settings/*`, `qr/*` | см. находки |
| Зависимости (Rust) | `cargo audit` (cargo-audit 0.8+, база 1527 advisories, 686 crates) | **6 уязвимостей, 10 warning** (раздел 4) |
| Зависимости (JS) | `pnpm audit --prod` | чисто |
| Грep по опасным паттернам | `dangerouslySetInnerHTML`, `innerHTML`, `eval(`, `new Function`, `document.write`, `window.open` | **ни одного вхождения** |
| Разрешения Tauri | `capabilities/*.json`, `gen/schemas/acl-manifests.json`, исходники `tauri-plugin-opener` | scope opener = только `http/https/mailto/tel` (ок) |
| Мобильная платформа | `gen/android/app/src/main/AndroidManifest.xml`, `tauri.properties` | см. SEC-10 |
| Проверка гипотез | срез `SessionManager`, `run_invoke_handler` из tauri 2.12.4, `aes` 0.8.4 (`cpufeatures` autodetect), `keepass` 0.17 (KDF без лимитов) | подтверждено в тексте |

Ограничения: нет динамического анализа (frida/objection), не проверялись подписанные сборки и CI-артефакты, нет доступа к iOS-коду (его нет в проекте).

---

## 2. Модель угроз

| № | Атакующий | Что может | Что защищает сейчас |
|---|---|---|---|
| T1 | Локальный пользователь/процесс с доступом к диску | читает `pms.vault`, копии, attachments | Argon2id (65 МиБ/3), XChaCha20-Poly1305, HKDF на сейв |
| T2 | Атакующий в LAN | видит mDNS/UDP-анонс, шлёт POST на порт приёма | 6-значный код, шифрованный архив |
| T3 | Файл-приманка (импорт KDBX/1PUX/Bitwarden/CSV/XML, QR, картинка) | подсовывает злонамеренный файл | лимиты только на размер входного файла |
| T4 | Компромисс webview (supply-chain npm, 0-day в WebKit) | вызывает **любую** `invoke`-команду | CSP, отсутствие HTML-инъекций |
| T5 | Кто-то с физическим доступом к разблокированному устройству | читает экран/буфер/ключницу | автоблокировка, FLAG_SECURE (Android) |
| T6 | Цепочка поставок Rust/npm | исполнение чужого кода | Cargo.lock/pnpm-lock, нет аудита в CI |

Ключевой вывод: сильная криптография (T1) и гигиена webview (отсутствие XSS) — на хорошем уровне. Слабые места — **границы доверия между webview и Rust** (T4), **парсеры недоверенных файлов** (T3), **локальная сеть** (T2) и **время жизни секретов в памяти**.

---

## 3. Сводная таблица

| ID | Серьёзность | Область | Кратко |
|---|---|---|---|
| SEC-01 | **High** | Tauri IPC | `storage_*` даёт webview полный доступ к ФС (чтение/запись/удаление, любой путь) |
| SEC-02 | **High** | Зависимости | `quick-xml` 0.38.4: RUSTSEC-2026-0194/0195 (7.5) — DoS на импорте KeePass XML |
| SEC-03 | **High** | Импорт | KDF-параметры из импортируемого файла не ограничены (Argon2 memory/iterations) → OOM/зависание |
| SEC-04 | **High** | Импорт | zip-бомбы: `read_to_end` без бюджета на распакованные данные (1PUX/Bitwarden .zip) |
| SEC-05 | Medium‑High | Восстановление | `pms_import` создаёт `pms.vault`/вложения с umask-правами (0644/0755) |
| SEC-06 | Medium | Биометрия (macOS) | vault key в Keychain без `SecAccessControl`; читается без участия пользователя |
| SEC-07 | Medium | Память | Секреты не зануляются: `VaultData`, `VaultSession.payload`, passphrase-DTO |
| SEC-08 | Medium | LAN-передача | 6-значный код без срока/lockout, plaintext HTTP, анонс устройства всем |
| SEC-09 | Medium | Аутентификация | Нет серверного счётчика попыток/задержки (биометрия — только в UI) |
| SEC-10 | Medium | Android | `allowBackup` не выключен, нет `dataExtractionRules` |
| SEC-11 | Medium | Приватность | Домены из хранилища уходят в Google (favicon), mDNS анонсирует имя устройства |
| SEC-12 | Medium | Ввод | keyfile без лимита размера → декодирование/хеширование произвольного объёма |
| SEC-13 | Medium | Concurrency | Мьютекс сессии держится всё время CPU-работы (AEAD 100 МиБ, сериализация) |
| SEC-14 | Medium | Supply chain | `cargo audit` не в CI; `tokio 1.54.0` (race), `rustix 1.1.5`, `crypto-bigint`, `ctutils`, yanked-крейты |
| SEC-15 | Low | CSP | `frame-src blob:` + `img-src blob:` — единственный «мост» к RCE при появлении iframe без `sandbox` |
| SEC-16 | Low | SSH | `ssh_install` пишет приватный ключ в произвольный каталог |
| SEC-17 | Low | Формат | Список KDF-слотов не входит в AAD полезной нагрузки |
| SEC-18 | Low | Буфер обмена | Нет «concealed»-маркеров → пароли попадают в историю буфера/менеджеры |
| SEC-19 | Info | Крипто | `rsa 0.9.10` (Marvin, RUSTSEC-2023-0071) — путь расшифровки не используется |
| SEC-20 | Info | Парсеры | Нет фаззинга парсеров недоверенных форматов |

---

## 4. Уязвимые зависимости (`cargo audit`, 18.11.2026)

| Крейт | Версия | Advisory | Затронуто в PMS | Действие |
|---|---|---|---|---|
| `quick-xml` | 0.38.4 | RUSTSEC-2026-0195 (high 7.5) — неограниченные аллокации в `NsReader` | `core/import_keepass_xml.rs` — **недоверенный ввод** | обновить до 0.42 (см. ниже) |
| `quick-xml` | 0.38.4 | RUSTSEC-2026-0194 (high 7.5) — квадратичное время на дубликатах атрибутов | там же | там же |
| `tokio` | 1.54.0 | RUSTSEC-2026-0384 — гонка в `time::Interval` | через `reqwest` (favicon) | `cargo update -p tokio --precise 1.54.1` |
| `crypto-bigint` | 0.5.5 | RUSTSEC-2026-0387 — вариативность по времени | через `p256/p384/p521/rsa/ecdsa` (генерация и разбор ключей) | обновить семейство RustCrypto |
| `ctutils` | 0.4.3 | RUSTSEC-2026-0385 — неконстантные маски | там же | обновить |
| `rsa` | 0.9.10 | RUSTSEC-2023-0071 (5.9, Marvin) | только генерация/сериализация, **расшифровки нет** | принять риск, задокументировать в `deny.toml` |
| `glib` | 0.18.5 | RUSTSEC-2024-0429 (unsound, Linux) | транзитивно через tauri/gtk | обновится с tauri; для Linux принять |
| `rustix` | 1.1.5 | RUSTSEC-2026-0486 (unsound xattr) | транзитивно (tempfile/async-io) | `cargo update -p rustix --precise 1.1.6` |
| `async-executor`, `clipboard-win`, `tokio` | — | yanked | — | обновляется вместе с tokio |
| `chrono` | 0.4.45 | unmaintained | форматирование дат | принять, следить |

Хорошая новость: в `Cargo.lock` уже есть `quick-xml 0.42.0` (его тянет `keepass 0.17`). Обновление нашей прямой зависимости до `0.42` **дедуплицирует** дерево и убирает уязвимую версию целиком:

```toml
# src-tauri/Cargo.toml
quick-xml = "0.42"
```

Код в `import_keepass_xml.rs` уже использует `Reader::from_reader`, `read_event_into`, `config_mut().trim_text(true)` — API стабилен, правок минимум (проверить `Event::Bytes`/`decode`).

---

## 5. Находки подробно

### SEC-01 · High · Полный доступ к файловой системе через `storage_*`

`src-tauri/src/commands/storage.rs:100-175`

```rust
#[tauri::command] pub fn storage_read_file(path: String) -> ApiResult<ReadFileResponse>   // любой файл
#[tauri::command] pub fn storage_write_atomic(path: String, b64: String, mode: Option<u32>)
#[tauri::command] pub fn storage_remove_file(path: String)
#[tauri::command] pub fn storage_remove_dir(path: String)                                  // remove_dir_all
#[tauri::command] pub fn storage_list_dir(path: String)
```

Пути приходят из фронтенда как строки и не проверяются ни на `..`, ни на принадлежность хранилищу. `mode` тоже приходит из фронтенда (можно поставить `0777`).

**Почему это важно.** Сегодня единственная преграда между потенциальным исполнением кода в webview и ОС — CSP (строгий, `script-src 'self'`) и отсутствие HTML-инъекций. Это одна линия обороны. При компромиссе (уязвимость в WebKit/WebView2, злонамеренный npm-пакет, будущая фича с `dangerouslySetInnerHTML`) атакующий получает:

* чтение любых файлов пользователя (`~/.ssh/id_*`, `~/.aws/credentials`, браузерные профили);
* запись в `~/.ssh/authorized_keys`, `~/Library/LaunchAgents/*.plist`, `~/.zshrc`, `~/Library/Application Support/<Tauri-app>` → **RCE с правами пользователя**;
* `storage_remove_dir` → удаление произвольного каталога.

Остальные команды с путями (`pms_import`, `transfer_install/send`, `ssh_install`, `attachment_*` через фронт) добавляют к этому чтение/запись в произвольные места.

**Как убрать.** Ввести в Rust «скоупы» — единственный источник прав на ФС:

```rust
// commands/fs_scope.rs
pub struct FsScope { roots: Mutex<Vec<PathBuf>> }   // добавляется в AppState

impl FsScope {
    /// Регистрирует корень только после нативного выбора папки пользователем.
    pub fn grant(&self, path: PathBuf) -> ApiResult<()>;      // canonicalize + push
    pub fn resolve(&self, raw: &str) -> ApiResult<PathBuf>;    // canonicalize + starts_with(root)
}
```

1. Диалог выбора папки/файла перенести на сторону Rust: `tauri_plugin_dialog::DialogExt::blocking_pick_folder` (desktop) и `tauri-plugin-android-fs` (Android) → `grant()` вызывается **только** на результат диалога, фронтенд не может «разрешить» себе путь.
2. Каждая `storage_*` команда начинается с `let path = state.fs_scope.resolve(&path)?;` — `canonicalize()` (отсекает `..`, симлинки наружу) + `starts_with` по одному из корней.
3. `mode` — не число извне, а enum: `PrivateFile | PrivateDir | PublicFile` → 0600/0700/0644. Или allowlist `[0o600, 0o700, 0o644, 0o755]`.
4. `storage_remove_dir` — только внутри `app_cache_dir()` и не глубже 2 уровней.
5. `pms_import(archive, destination)`: `archive` — только через нативный file-picker (в Rust), `destination` — только внутри грантованного корня.
6. `transfer_send(vault_path)` — брать путь из `vaultPath` текущей сессии на стороне Rust, а не из параметра.

**Критерии приёмки**
- `tests/` в Rust: `resolve("/etc/passwd")` → `Err`; `resolve(root/../escape)` → `Err`; симлинк из корня наружу → `Err`.
- Playwright/e2e: `invoke("storage_read_file", {path:"/etc/hosts"})` из консоли webview → ошибка `fs_scope`.
- Функционал (сохранение, бэкапы, вложения, экспорт/импорт, обмен) не регрессирует.

---

### SEC-02 · High · `quick-xml` 0.38.4 (два advisory 7.5)

См. таблицу выше. Импорт KeePass XML (`import_keepass_xml.rs`) читает XML, который приходит из файла, выбранного пользователем; RUSTSEC-2026-0195 = «неограниченная аллокация объявлений пространств имён», RUSTSEC-2026-0194 = «квадратичное время на дубликатах атрибутов». Оба достижимы одним 2-килобайтным файлом.

**Как убрать:** `quick-xml = "0.42"`; в CI добавить `cargo audit` как блокирующий шаг (SEC-14), чтобы это не повторилось.

---

### SEC-03 · High · KDF-параметры из импортируемого файла не ограничены

* `src-tauri/src/core/import_bitwarden.rs:114` — `let iterations = envelope.kdf_iterations.unwrap_or(600_000) as u32;`
  * `u64 → u32` **срезается молча**: `4_294_967_296 + 100_000` превратится в `100_000` (несовпадение пароля вместо ошибки), а `10^9` итераций — это часы работы.
* `:120` — `let memory = envelope.kdf_memory.unwrap_or(64) as u32;` → `memory * 1024` без верхней границы: файл с `kdfMemory = 8_388_608` требует 8 ГиБ.
* `src-tauri/src/core/import_keepass.rs:26` — `Database::parse(bytes, key)`: `keepass 0.17` передаёт `memory/1024` в `argon2::Params` без проверок (`keepass-0.17.0/src/crypt/kdf.rs:80`). KDBX-заголовок — тоже недоверенный ввод.

**Почему важно:** T3 — «файл-приманка». Открытие бэкапа от «коллеги» вешает приложение или роняет его по OOM (на Android это ещё и убийство процесса системой).

**Как убрать:**

```rust
const MAX_KDF_MEMORY_KIB: u32 = 1 << 20;         // 1 ГиБ
const MAX_KDF_ITERATIONS: u32 = 10_000_000;

fn checked_kdf(iterations: Option<u64>, memory_kib: Option<u64>) -> Result<(u32, u32), VaultError> {
    let it = u32::try_from(iterations.unwrap_or(600_000)).map_err(|_| VaultError::ImportCorrupt)?;
    let mem = u32::try_from(memory_kib.unwrap_or(64)).map_err(|_| VaultError::ImportCorrupt)?;
    if it > MAX_KDF_ITERATIONS || mem > MAX_KDF_MEMORY_KIB { return Err(VaultError::ImportCorrupt); }
    Ok((it, mem))
}
```

Для KeePass: до `Database::parse` разобрать VariantDictionary заголовка KDBX4 (поле `0x0B`, ключ `M`) и отклонить `M > 1 ГиБ` / `I > 10^7`; либо перейти на `keepass`-API с собственными `Kdf` параметрами, если появится. Импорт выполнять в `spawn_blocking` с дедлайном.

**Критерии приёмки:** тесты с фикстурами «kdfMemory=8388608», «kdfIterations=10^10», «u64→u32 wrap» → `import_corrupt` за < 100 мс, RSS процесса не растёт.

---

### SEC-04 · High · zip-бомбы при импорте архивов

`src-tauri/src/core/import.rs:343,361` и `core/import_onepux.rs:65` — `entry.read_to_end(&mut content)` без бюджета. Ограничение есть только на **входной** файл (`commands/import.rs:20`, 500 МБ). ZIP «1000:1» из 500 МБ даёт 500 ГиБ распаковки.

**Как убрать:** бюджет на распакованные данные (например 1 ГиБ суммарно, 200 МБ на файл) и чтение через `entry.take(budget)`; проверять `entry.size()` **до** `Vec::with_capacity`. В `pms_archive::import:206` уже есть `Vec::with_capacity(entry.size() as usize)` по недоверенному `size` — заменить на `min(size, budget)`.

---

### SEC-05 · Medium-High · Восстановление архива оставляет файлы «читаемыми»

`core/pms_archive.rs:212` — `File::create(&target)` (права = `0666 & ~umask` → обычно **0644**), `:110` — то же для самого архива при экспорте. Команда `pms_import` (`commands/pms_archive.rs:14`) никаких `chmod` не делает. На десктопе `importVaultArchive` (`src/lib/storage/archive.ts:92`) идёт именно этим путём (ветка Android отдельная). Сравните: `transfer_install` (`commands/transfer.rs:98-157`) права выставляет через `secure_file/secure_dir` — то есть в одном месте забыли.

**Как убрать:** `OpenOptions::new().write(true).create(true).truncate(true).mode(0o600)` (unix) / `create_new` + ACL (windows), каталоги 0700; в конце `import()` — `chmod` на `pms.vault` и `attachments/`. Тест: после `import()` права 0600/0700 (как в `transfer::install_new_archive_restores_vault_and_attachments`).

---

### SEC-06 · Medium · macOS-биометрия: ключ в Keychain без Access Control

`src-tauri/src/commands/biometric.rs:226-232`

```rust
let options = PasswordOptions::new_generic_password(SERVICE, &account);
set_generic_password_options(&secret, options)          // обычный generic password
```

и `:262-271` — `generic_password(...)` (чтение без участия пользователя; в файле есть даже тест `stores_and_reads_key_without_a_biometric_prompt`, строка 323).

Проверка `LAContext` живёт **в приложении** (`evaluate()`, строка 197), а не в Keychain. Следствия:

1. Ключ хранилища можно прочитать, не проходя биометрию, — из любого кода, исполняющегося в процессе PMS (отладчик, инъекция, будущая ошибка «прочитать ключ для статуса»). Защита сводится к дисциплине кода.
2. Нет `kSecAttrAccessible…ThisDeviceOnly` → элемент попадает в шифрованные локальные бэкапы устройства.
3. Android-ветка (`biometric_cipher`) сделана правильнее: шифротекст на диске, ключ в Keystore, пользователь обязателен.

**Как убрать:** `SecAccessControl` с `kSecAccessControlBiometryCurrentSet | kSecAccessControlOr | kSecAccessControlDevicePasscode` и `kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly`; тогда системный запрос биометрии возникает на самом `SecItemCopyMatching`, а явный `evaluate()` становится не нужен (или остаётся только для UX-сообщений) — ОС требует присутствия пользователя, и украденный `login.keychain-db` бесполезен.

Важно: `security_framework::passwords::PasswordOptions` **не умеет** access control (в 3.7.0 есть только `set_generic_password_options`), поэтому элемент надо писать напрямую через `security_framework_sys` (он уже в зависимостях):

```rust
use security_framework::access_control::{ProtectionMode, SecAccessControl};
use security_framework_sys::access_control::{
    kSecAccessControlBiometryCurrentSet, kSecAccessControlDevicePasscode, kSecAccessControlOr,
};
use security_framework_sys::item::{kSecAttrAccessControl, kSecAttrAccount, kSecAttrService, kSecClass, kSecClassGenericPassword, kSecValueData};

let ac = SecAccessControl::create_with_protection(
    Some(ProtectionMode::AccessibleWhenPasscodeSetThisDeviceOnly),
    kSecAccessControlBiometryCurrentSet | kSecAccessControlOr | kSecAccessControlDevicePasscode,
)?;
// CFDictionary { kSecClass: genericPassword, kSecAttrService, kSecAttrAccount,
//                kSecAttrAccessControl: ac, kSecValueData: key } → SecItemAdd / SecItemUpdate
```

Миграция: прочитать старый элемент (без AC), записать новый с AC, удалить старый; `SecItemUpdate` с `kSecAttrAccessControl` требует удаления+вставки (Apple это запрещает менять на месте).

---

### SEC-07 · Medium · Секреты не зануляются

* `core/vault.rs:23-28` — `VaultSession.payload: Vec<u8>` (шифротекст) и `header` живут до локa; ключ зануляется (`Zeroizing`) — хорошо.
* `core/models.rs` — `Entry.password`, `TotpConfig.secret`, `SshKey.private_key`, `BankCard` — обычные `String`/`Vec<u8>`, `Drop` не зануляет; `save_vault` сериализует их в `Zeroizing<Vec<u8>>`, но **исходная структура** остаётся в куче до переаллокации (может попасть в swap/дамп/бэкап памяти).
* `commands/vault.rs:17-32` — `UnlockSecretDto` держит парольную фразу в `String` без зануления; фронтенд держит её в React-state (неизбежно).

**Как убрать:** `#[derive(Zeroize, ZeroizeOnDrop)]` на `VaultData`, `Entry`, `TotpConfig`, `HistoryItem`, `SshKey`, `BankCard`, `Document`, `Draft` (все поля `String/Vec<u8>/Option<T>` уже реализуют `Zeroize`); `Zeroizing<String>` в DTO и в `UnlockSecretInput`; `VaultSession` — реализовать `Drop` (занулить `payload`). Ограничение по памяти (`mlock`) — отдельная опция, для менеджера паролей скорее нет (риск OOM), но swap-риск стоит зафиксировать в документации.

---

### SEC-08 · Medium · Передача по LAN: 6 цифр, без срока, без lockout, plaintext

`commands/transfer.rs`
* `:339` — `format!("{:06}", rand::thread_rng().gen_range(0..1_000_000))` → ~20 бит.
* `:371-375` — неверный код: `sleep(250ms)` и `403`, **предела попыток нет**; 4 попытки/с → перебор 10⁶ за ~3 суток, и это ещё медленно только из-за sleep.
* `:324` — сервер на `0.0.0.0:0`; `:549` — код уходит в **открытом** HTTP-заголовке `X-PMS-Code` (любой в том же Wi-Fi с tcpdump перехватит и переиграет).
* `:341` + `register_service` (`:174`) — устройство анонсируется в mDNS/UDP всем в сети (`PMS · <hostname>`), отвечает на любой запрос.
* Нет срока жизни сессии и нет отзыва после неудач.

**Как убрать:**
1. Код = 8 символов Crockford-base32 (`~40 бит`), показывать группами; срок жизни 120 с, после — `transfer_receive_stop` + событие в UI.
2. Счётчик неудач в `ReceiverSession`: 3 → остановить приём, показать «попытки исчерпаны».
3. Аутентифицировать сам архив кодом: XChaCha20-Poly1305 с ключом `HKDF(code, salt)`, AAD = `vault_id`; тогда перехваченный трафик не даёт ни кода, ни переигровки, а «правильный» код проверяется тегом (сравнение константное).
4. `Server::http("0.0.0.0:0")` → bind на конкретный интерфейс (`local_address()`), если он один.
5. Не отдавать имя устройства в ответе discovery без активной сессии приёма.

---

### SEC-09 · Medium · Нет ограничения попыток на стороне бэкенда

`core/vault.rs:83-110` — единственная цена попытки — Argon2 (~0.3–0.6 с). Для парольных фраз это приемлемо, но: (а) `biometric_unlock` (`commands/biometric.rs:80-129`) вообще без счётчика — 5 попыток считает только `UnlockScreen.tsx:54` (`bioAttemptsLeft`), сбрасывается перезагрузкой страницы/приложения; (б) нет задержки/капчи/«заморозки» после серии неудач.

**Как убрать:** счётчик неудач в `AppState` с сохранением в `app_data_dir/unlock-state.json` (monotonic + время), экспоненциальная задержка (`1s, 2s, 4s … 5 min`), для биометрии — после N неудач требовать парольную фразу и не давать `biometric_unlock` вообще.

---

### SEC-10 · Medium · Android-бэкап включён

`src-tauri/gen/android/app/src/main/AndroidManifest.xml` — нет `android:allowBackup`, нет `android:dataExtractionRules`, нет `android:fullBackupContent`. По умолчанию `allowBackup="true"` (API < 31) → `adb backup`/Google Backup уносят `app_data_dir`, включая `biometric/<vault_id>.bin` (хоть и зашифрованный Keystore-ключом устройства — на другом устройстве не расшифруется, но лучше не давать). Также нет `networkSecurityConfig` (cleartext нужен только для LAN-передачи приёма — её стоит ограничить).

**Как убрать:**
```xml
<application android:allowBackup="false"
             android:fullBackupContent="false"
             android:dataExtractionRules="@xml/data_extraction_rules" …>
```
`res/xml/data_extraction_rules.xml` — `cloud-backup`/`device-transfer` с `<exclude domain="file" path="."/>` (кроме, если нужно, экспортов пользователя).

---

### SEC-11 · Medium · Приватность: Google и mDNS

* `commands/favicon.rs:8` — `https://www.google.com/s2/favicons?domain=…`: каждый уникальный домен из хранилища уходит в Google; `settings.faviconFetch` по умолчанию `true` (`core/models.rs:112`), переключатель спрятан в настройках.
* Кэш иконок — `localStorage` (`faviconCache.ts`), т.е. виден любому скрипту webview.
* mDNS/UDP-анонс имени устройства (`transfer.rs:174-190`).

**Как убрать:** по умолчанию выключить; при первом включении — явное предупреждение «домены будут отправляться в Google»; либо тянуть `https://<host>/favicon.ico` напрямую (утечка идёт только на сам сайт, как в браузере) с лимитом 512 КБ и таймаутом (всё уже есть в `favicon.rs`); кэш — в IndexedDB и чистить при `lock`.

---

### SEC-12 · Medium · keyfile без лимита размера

`commands/vault.rs:40-62` — `bytes_b64` декодируется целиком, затем `canonical_secret` → SHA-256 по всему буферу (`core/crypto.rs:280-300`), плюс попытки `ssh_key::PrivateKey::from_openssh(text)` / PEM-парсеры на произвольном тексте. Ключевой файл в 500 МБ (или «мусор», который парсится в SSH-ключ) — лишняя память и время.

**Как убрать:** `MAX_KEYFILE_BYTES = 1 * 1024 * 1024` — проверять до декодирования (`bytes_b64.len()`), и в `canonical_secret` тоже (на случай прямого вызова).

---

### SEC-13 · Medium · Мьютекс сессии держится во время CPU-работы

`core/session.rs:47-61` — `with_mut` держит `Mutex<Option<VaultSession>>` и выполняет переданную замыканием работу. В `vault_save` это сериализация всего `VaultData` + AEAD + base64; в `attachment_seal` (`commands/attachments.rs:61`) — AEAD до 100 МБ. Всё это время **любая** другая команда (включая `vault_lock`!) ждёт.

Практические следствия: «залипший» UI при сохранении большого хранилища, невозможность прервать операцию, и возможность DoS: одна команда с 100 МБ вложением блокирует блокировку/сохранение/чтение.

**Как убрать:** `SessionManager::key(session_id) -> Zeroizing<[u8;32]>` (копия 32 байт под мьютексом, отпустить, работать), а результат сохранять отдельным коротким `with_mut`-обновлением (`set_payload`). Дополнительно — перевести тяжёлые команды в `async` + `spawn_blocking` (см. PERF-05).

---

### SEC-14 · Medium · Supply chain и CI

* `.github/workflows/build-all.yml` — только сборка; `cargo audit`/`cargo deny`/`pnpm audit` не запускаются. `actions/checkout@v4`, `pnpm/action-setup@v4`, `actions/setup-node@v4` прибиты тегом, а не SHA.
* 686 crates в `Cargo.lock` (для менеджера паролей это большая поверхность; каждый `cargo update` — потенциальный `build.rs` с произвольным кодом).
* Найдено `cargo audit`: см. раздел 4 (tokio race, rustix unsound, crypto-bigint/ctutils timing, yanked).

**Как убрать:**
```yaml
- uses: actions/checkout@<sha>          # пин по commit SHA
- run: cargo install cargo-audit cargo-deny --locked
- run: cargo deny check advisories bans sources licenses   # deny.toml в репо
- run: cargo audit
- run: pnpm audit --prod --audit-level=high
```
`deny.toml`: `[advisories] ignore = ["RUSTSEC-2023-0071"]` с комментарием «rsa используется только для генерации/сериализации, расшифровки нет»; `[bans] multiple-versions = "warn"`.
Плюс: `cargo machete` (неиспользуемые зависимости), `cargo vet`/`cargo crev` — по желанию.

---

### SEC-15 · Low · CSP: `frame-src blob:` / `img-src blob:`

`src-tauri/tauri.conf.json`:
```
default-src 'self'; img-src 'self' asset: http://asset.localhost data: blob:; …
frame-src blob:; object-src 'none'; base-uri 'none'; form-action 'none'
```
Это минимум, необходимый для предпросмотра PDF (`AttachmentPreviewModal.tsx:150`, iframe `sandbox=""` — правильно) и картинок. Опасность в будущем: любой новый `<iframe src={blobUrl}>` **без** `sandbox` = исполнение скрипта в origin приложения, т.е. полный доступ к `invoke` (и к SEC-01). `asset:` в CSP разрешён, но `assetProtocol` не сконфигурирован — можно убрать.

**Как убрать:** оставить `frame-src blob:` при условии, что единственный потребитель — PDF-превью с `sandbox=""`; добавить комментарий в `tauri.conf.json` и e2e-проверку, что `iframe` в превью имеет `sandbox=""`; удалить `asset:` из CSP.

---

### SEC-16 · Low · `ssh_install` — произвольный каталог

`commands/ssh.rs:142-188` — имя файла валидируется (`safe_name`), каталог — нет. Запись приватного ключа в произвольное место (и `create_dir_all` там же). В связке с SEC-01 не усиливает атакующего, но после ввода скоупов должно попасть под тот же `resolve()`.

---

### SEC-17 · Low · Заголовок хранилища аутентифицирован не полностью

`core/crypto.rs:249-258` (`header_core_aad`) связывает `format`, `vault_id`, `cipher`, `created_at`, но **не** список слотов. Каждый слот аутентифицирован своим AAD (`slot_aad`), поэтому удалить/подменить слот без ключа нельзя (переупаковка vault-key требует KEK). Однако «удаление слота» обнаруживается только по UX («неверные данные»/«нет метода»), а не как «файл изменён». После ввода мультислотов (passphrase + keyfile) стоит положить в AAD дайджест набора слотов (`SHA-256` от `(id, kind)` в стабильном порядке) — тогда любое изменение состава слотов ломает расшифровку payload и диагностируется явно.

---

### SEC-18 · Low · Буфер обмена: нет «concealed»-маркеров

`src/lib/clipboard.ts` — очистка best-effort и только если значение не изменилось (хорошо). Но пока пароль лежит в буфере, он попадает в: историю Windows (`Win+V`), менеджеры буфера (Raycast/Alfred/Paste/Maccy), синхронизацию Universal Clipboard (macOS↔iOS). Платформенные маркеры решают это: macOS `org.nspasteboard.ConcealedType` + `TransientType`, Windows формат `ExcludeClipboardContentFromMonitorProcessing` / `CanIncludeInClipboardHistory=0`.

**Как убрать:** команда `clipboard_copy_secret(value)` на Rust с `#[cfg(target_os)]` реализацией маркеров, вызвать её вместо `writeText` для секретов (текст/логины — можно оставить плагин).

---

### SEC-19 · Info · `rsa` (Marvin) — не эксплуатируется

`rsa 0.9.10` тянется напрямую и через `ssh-key`. Все пути (`core/crypto.rs:402-441` — конвертация ключей, `core/ssh.rs:52-112` — генерация, `commands/ssh.rs:57-99` — экспорт форматов) работают с **приватными ключами локально**, расшифровки чужих шифротекстов (где Marvin применим) нет. Оставить, зафиксировать в `deny.toml`; следить за `rsa 0.10`.

---

### SEC-20 · Info · Нет фаззинга парсеров

Недоверенный ввод: `vault_file::parse_vault_file`, `import::parse` (KDBX/XML/CSV/1PUX/Bitwarden), `pms_archive::import` (zip), `share::decode` (zlib+base64+AEAD), `totp::inspect`, `ssh::formats`, `favicon::normalize_host`. Часть уже покрыта лимитами, но SEC-03/04 показывают, что лимиты добавляются по мере обнаружения.

**Как убрать:** `cargo fuzz` с таргетами на каждый парсер (+ `-Z sanitizer=memory` ночные прогоны в CI). Это дешёвая страховка от повторения SEC-03/04.

---

## 6. Что сделано хорошо (не трогать)

* Крипто: Argon2id с разумными профилями и **проверкой границ** параметров (`crypto.rs:63-84`), KEK→wrap XChaCha20-Poly1305, HKDF-разделение ключей на каждый payload/вложение (`crypto.rs:263`, `attachments.rs:135`), AAD связывает `vault_id`/слот/размер; NFC-нормализация парольной фразы; каноникализация keyfile (SSH/PKCS8 дают одинаковый секрет — есть тесты); `OsRng`.
* Формат: строгие проверки magic/длины/лимитов, `checked_add`, отказ от trailing-мусора, дедуп id/kind слотов, отказ `payload_len > 512 МиБ` (`vault_file.rs`).
* Архив: манифест с SHA-256 каждого файла, `safe_path` против traversal, отказ от непустого приёмника, откат при ошибке (`pms_archive.rs`).
* Атомарная запись (tempfile + `sync_all` + rename) и явные 0600/0700 в `storage_write_atomic` и `ssh_install`; `icacls` на Windows.
* Webview: CSP без `unsafe-eval` в prod, `script-src 'self'`, `object-src 'none'`, `base-uri 'none'`, `form-action 'none'`; нет `withGlobalTauri`; ни одного `innerHTML`/`dangerouslySetInnerHTML`; PDF — в `sandbox=""`; SVG не превьюится; mime для превью нормализуется (`preview.ts`); загрузка файлов идёт через нативный диалог.
* Tauri capabilities узкие (`core:default` + dialog + clipboard-write + opener с http/https/mailto/tel); remote-контент не может вызывать `invoke` (нет `remote`-домена).
* Парсеры и ядро не паникуют на недоверенном вводе: в боевом коде `core/`/`commands/` **нет** `unwrap`/`expect`/`panic!` (все 343 вхождения — в `#[cfg(test)]`), индексация защищена предварительными проверками длины.
* Секреты: ключ хранилища — `Zeroizing`, `VaultError` не различает «неверный пароль» и «битый слот», лимиты на размер вложений/импорта/QR, константное сравнение MAC при импорте Bitwarden, отказ публичным ключам в keyfile.
* Нет автообновления/телеметрии/сетевых вызовов, кроме favicon (по кнопке настройки) и LAN-обмена.

---

## 7. План устранения

### P0 — 1–2 дня (безопасность ядра и данных)

| # | Задача | Файлы | Приёмка |
|---|---|---|---|
| 1 | `FsScope` + перенос диалогов в Rust; `resolve()` в каждой `storage_*`, `pms_*`, `ssh_install`, `transfer_*` | `commands/storage.rs`, `pms_archive.rs`, `ssh.rs`, `transfer.rs`, `lib.rs`, `AppState` | SEC-01 критерии; e2e «invoke с чужим путём → ошибка» |
| 2 | `quick-xml = "0.42"`, `cargo update -p tokio --precise 1.54.1`, `rustix 1.1.6` | `Cargo.toml`, `Cargo.lock` | `cargo audit` — 0 vuln (кроме задокументированного `rsa`) |
| 3 | Лимиты KDF при импорте (checked cast + caps) | `core/import_bitwarden.rs:114,120`, `core/import_keepass.rs` | фикстуры из SEC-03 падают быстро |
| 4 | Бюджет распаковки zip | `core/import.rs:343,361`, `core/import_onepux.rs:65`, `core/pms_archive.rs:206` | zip-бомба 1000:1 отклоняется за < 1 с |
| 5 | Права 0600/0700 при `pms_archive::import/export` | `core/pms_archive.rs:110,212` | тест прав после импорта |
| 6 | Лимит keyfile 1 МиБ | `commands/vault.rs:40-62`, `core/crypto.rs` | тест «10 МиБ keyfile → ошибка» |

### P1 — неделя (модель угроз T2/T3/T5)

| # | Задача | Приёмка |
|---|---|---|
| 7 | Биометрия macOS: `SecItemAdd` с `SecAccessControl` (`BiometryCurrentSet \| Or \| DevicePasscode`, `WhenPasscodeSetThisDeviceOnly`), миграция старого элемента, убрать зависимость от `evaluate()` | ключ читается только после системного запроса; старый элемент удалён |
| 8 | LAN-передача: 8-символьный код, TTL 120 с, 3 попытки, AEAD архивa кодом, bind на интерфейс | перебор и перехват кода бесполезны; e2e обмена зелёный |
| 9 | Серверный счётчик неудач + экспоненциальная задержка (пароль и биометрия) | 10 неудач → задержка ≥ 30 с |
| 10 | Android `allowBackup=false` + `dataExtractionRules` | `adb backup` не содержит app_data |
| 11 | `Zeroize`/`ZeroizeOnDrop` на моделях и DTO; `Drop` у `VaultSession` | `cargo test` + ревью |
| 12 | `SessionManager::key()` — не держать мьютекс во время крипто | нагрузочный тест: `vault_lock` во время seal 100 МиБ отвечает < 50 мс |
| 13 | favicon: по умолчанию выключено, предупреждение, кэш в памяти/IndexedDB с очисткой при lock | нет запросов к Google без согласия |

### P2 — 2–3 недели (гигиена и глубина)

| # | Задача |
|---|---|
| 14 | CI: `cargo audit` + `cargo deny` + `pnpm audit` + пин actions по SHA + `deny.toml` |
| 15 | `cargo fuzz` на 7 парсеров + ночной прогон |
| 16 | Clipboard «concealed»-маркеры (macOS/Windows) |
| 17 | Дайджест набора слотов в `header_core_aad` (SEC-17) |
| 18 | Убрать `asset:` из CSP, комментарии/тест на `sandbox=""` в превью |
| 19 | Разбор `rsa` → `0.10`/`ssh-key` без `rsa`, когда появится; контроль `crypto-bigint`/`ctutils` |
| 20 | Документ «модель угроз и остаточные риски» в `docs/`, включая «JS-куча не зануляется» |

---

## 8. Чек-лист проверки после исправлений

```bash
cd src-tauri && cargo audit && cargo deny check advisories bans sources
cd src-tauri && cargo test --release            # + новые тесты SEC-01/03/04/05/12
cargo test --release --lib perf_audit -- --ignored --nocapture --test-threads=1
pnpm audit --prod && pnpm typecheck && pnpm test && npx playwright test
# ручное: импорт-бомба KDBX/XML/zip, keyfile 10 МиБ, обмен по LAN с неверным кодом 10 раз,
#         чтение /etc/passwd из консоли webview, adb backup на Android, биометрия macOS
```
