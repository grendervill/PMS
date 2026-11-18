# Повторный аудит безопасности PMS (после исправлений)

Дата: 19.11.2026 · Ветка: `main` + рабочее дерево (`388d641` + изменения) · Проверялось **по коду**, а не по прошлым выводам.

Документов два: этот — безопасность, `2026-11-19-perf-audit-2.md` — производительность. Предыдущие: `2026-11-18-security-audit.md`, `2026-11-18-perf-audit.md`.

**Что сделано в этом раунде:** закрыты SEC-01…SEC-05, SEC-10, SEC-12…SEC-14; независимое ревью (`reviewer`) нашло 14 проблем в моих же правках, из них 1 критическая и 3 высокие — все исправлены и покрыты тестами (раздел 4). Осталось 7 пунктов, разобранных в разделе 5 с оценкой риска.

---

## 1. Как проверялось

| Шаг | Что | Результат |
|---|---|---|
| Ручной разбор | все 51 команда `commands/*.rs`, всё ядро `core/*.rs` после правок: границы доверия webview→Rust, крипто-путь, парсеры, лимиты, async | см. раздел 2–5 |
| Независимое ревью | отдельный агент-ревьюер по полному `git diff` + старому аудиту | 14 находок, все разобраны |
| Зависимости | `cargo audit` (RustSec DB, 686 crates), `pnpm audit --prod` | **0 уязвимостей**, 8 warning/advisory без исправлений (раздел 3) |
| Грep-проверки | `resolve(` в каждой команде с путём; `unwrap/expect/panic` в боевом коде | все пути резолвятся; паник в production-коде нет |
| Тесты | 179 unit-тестов Rust (+8 новых на `FsScope`, +4 на KDF-лимиты, +1 на XML-сущности), 221 фронт, 27 e2e | зелёные |
| Поведенческие проверки | «бомбы» KDF на реальном парсере (раздел 6), права файлов, `fs-scope.json` с грантом `/` | отклоняются за <0.1 мс |

---

## 2. Что теперь реально закрыто

| ID | Было | Стало | Чем подтверждено |
|---|---|---|---|
| SEC-01 | `storage_*` давал webview полный доступ к ФС | `FsScope`: гранты только из нативного диалога, `canonicalize`+`starts_with`, запрет `..`, симлинков наружу, `mode` из allowlist, удаление корня запрещено | 14 unit-тестов в `commands/fs_scope.rs`; все команды с путём вызывают `resolve` (грep: storage 7, pms_archive 4, transfer 4, ssh 2, attachments 2, vault 1) |
| SEC-02 | `quick-xml 0.38.4` (2×7.5 DoS) | `0.41.0` (патченная версия; `cargo audit` чист). `Event::GeneralRef` теперь обрабатывается — иначе `&amp;` в KeePass-XML молча терялся (нашёл тестом) | `cargo audit`; тест `unescapes_entities_in_field_values`, `keeps_unknown_entities_literally` |
| SEC-03 | KDF-параметры из файла без границ (Argon2 `t`=10⁷, M=1 ГиБ) | Проверяется **работа** (`memory × iterations ≤ 2·10⁶`), отдельные лимиты для Argon2 (`t ≤ 64`) и PBKDF2 (10⁷), без `u64→u32` срезов; то же для KDBX (`M/1024 × I`) | тесты `rejects_kdf_parameter_bombs`, `rejects_work_bomb`; замер: бомба отклоняется за 0.01 мс |
| SEC-04 | zip-бомбы в 1PUX/Bitwarden | `UnpackBudget` (256 МиБ на запись, 1 ГиБ на архив), чтение через `take(declared)` — врущий `entry.size()` не даёт распаковать больше | `copy_and_hash`/`read_entry`, тесты архива |
| SEC-05 | восстановленный архив с правами 0644/0755 | `create_private_file` (0600) и `set_private_dir` (0700) на всех путях импорта/экспорта | тесты `export_import_roundtrip`, права проверяются в `transfer`-тестах |
| SEC-10 | Android-бэкап включён | `allowBackup=false`, `fullBackupContent=false`, `dataExtractionRules` с полным исключением | `AndroidManifest.xml`, `res/xml/data_extraction_rules.xml` |
| SEC-12 | keyfile без лимита | 1 МиБ, проверка **до** base64-декодирования (`decode_keyfile`) | `commands/vault.rs` |
| SEC-13 | мьютекс сессии держится во время AEAD/JSON | `SessionManager::material()` копирует ключ (Zeroizing) и отпускает мьютекс; тяжёлые команды — `async` + `spawn_blocking` | `core/session.rs`, `commands/{vault,attachments,pms_archive,import}.rs` |
| SEC-14 | нет аудита в CI, actions по тегам | job `security` (cargo-audit, cargo-deny, pnpm audit), `deny.toml`, `.cargo/audit.toml` с обоснованными ignore, все actions прибиты по commit SHA | `.github/workflows/build-all.yml` |
| SEC-16 | `ssh_install` в произвольный каталог | путь через `resolve`; `chmod 0700` только для созданного каталога или `.ssh`-подобного имени | `commands/ssh.rs` |

Дополнительно закрыто по ходу (из ревью): лимит `MAX_PAYLOAD_LEN` при сохранении, проверка `vault_id` в боевом пути сохранения, гонка save/rewrap (`SessionChanged`), размер вложения/импорта проверяется до чтения и до base64, экспорт архива падает при изменении файла между проходами, JSON-метаданные импорта ≤64 МиБ, `grant_limit` вместо тихого вытеснения грантов.

---

## 3. Зависимости (состояние на 19.11.2026)

```
$ cargo audit          → 0 vulnerabilities, 8 warnings
$ pnpm audit --prod    → No known vulnerabilities found
```

Остаточные предупреждения и решение:

| Крейт | Advisory | Решение |
|---|---|---|
| `quick-xml` 0.41.0 + 0.42.0 | — (обе версии патчены) | две версии в дереве: наша 0.41 и 0.42 у `keepass`. Дедуп невозможен без перехода `keepass`; `deny.toml` держит `multiple-versions = "warn"` |
| `rsa` 0.9.10 | RUSTSEC-2023-0071 (Marvin) | ignore с обоснованием: только генерация и DER-сериализация, расшифровки чужого шифротекста нет |
| `crypto-bigint` 0.5.5, `ctutils` 0.4.3 | RUSTSEC-2026-0387 / 0385 (constant-time) | ignore с обоснованием; переход требует p256/p384/p521 0.14 + rsa 0.10 + pkcs8 0.11 одним шагом |
| `chrono`, `zbus*`, `proc-macro-error`, `glib` | unmaintained / unsound | транзитивные (tauri/gtk), обновляются вместе с tauri; на Linux принято |

Проверено по коду: в `Cargo.lock` нет `git`-зависимостей, `openssl`/`native-tls` запрещены в `deny.toml` (TLS только `rustls`).

---

## 4. Что нашло независимое ревью в моих правках (и как исправлено)

| # | Серьёзность | Проблема | Исправление |
|---|---|---|---|
| 1 | **Critical** | `fs-scope.json` лежал в `app_data_dir`, а тот был выдан webview как `ReadWrite` (на macOS/Windows config и data — один каталог). Renderer мог записать себе грант `/` → RCE, живучая между запусками | `app_data_dir` больше не грантится; каталог кэша — единственный автогрант; загруженные гранты валидируются (не `/`, не `$HOME`, не покрывают store/кэш, обязаны содержать `pms.vault`); в store пишутся только «папочные» гранты с vault'ом — подделка бесполезна даже при доступе к файлу. 8 тестов, включая `rejects_grant_store_content_that_grants_everything` |
| 2 | **High** (функц.) | `dialog_pick_save_file` не работал для новых файлов: `grant()` требует существования, а save-диалог возвращает ещё не созданный путь. Ломались «Экспорт архива» и «Скачать вложение» | `FsScope::grant_target()` (канонизирует ближайший существующий родитель), только на цель; моки e2e это маскировали — добавлен unit-тест |
| 3 | **High** | Лимит KDF проверял `memory` и `iterations` по отдельности → `M=1 ГиБ × I=10⁷` (годы работы) | Проверка произведения + отдельный `t ≤ 64` для Argon2 (см. SEC-03) |
| 4 | **High** | `vault_save` больше не проверял `data.id == header.vault_id` (проверка осталась в тестовой `save_vault`) → можно было записать нечитаемый файл | Проверка перенесена в `seal_document` (единственный путь) |
| 5 | Medium | `MAX_PAYLOAD_LEN` не проверялся при сохранении → файл писался и потом не открывался | Проверка в `seal_document`, `MAX_READ_BYTES` согласован с форматом (513 МиБ) |
| 6 | Medium | экспорт архива: хеширование и копирование — два прохода; файл мог измениться → «битый» бэкап, о котором узнаёшь при восстановлении | Второй проход хеширует при копировании и при расхождении удаляет архив с ошибкой |
| 7 | Medium | `attachment_open_file` читал файл целиком до проверки размера (4 ГиБ в `attachments/` → OOM) | `metadata().len()` + `take(limit)` |
| 8 | Medium | `import_parse`/`decode_file` декодировали base64 до проверки размера (4 ГБ аргумент → 3 ГБ памяти) | Проверка длины закодированной строки до декодирования |
| 9 | Medium | Bitwarden `data.json` до 256 МиБ → дерево `serde_json::Value` в 8–15× (2–4 ГиБ RSS) | Лимит JSON-метаданных 64 МиБ |
| 10 | Low | `ssh_install` делал `chmod 0700` любому грантованному каталогу | только созданному приложением или `.ssh`/`ssh` |
| 11 | Low | гонка `save` × `rewrap`: сохранение могло перезаписать новый заголовок старым | `generation` в сессии, `set_sealed` отклоняет устаревшее сохранение (`session_changed`) |
| 12 | Low | при 33-м гранте тихо выбрасывался самый старый (доступ к хранилищу терялся без сообщения) | `fs_scope_limit` вместо вытеснения |
| 13 | Low | диалог блокировал воркер рантайма до 600 с | таймаут 180 с |
| 14 | Info | `atomicwrites` создаёт temp-файл с umask до `chmod 0600` | принято: содержимое — шифротекст; зафиксировано в разделе 5 |

---

## 5. Остаточные риски (не закрыто, с оценкой)

| # | Риск | Почему не закрыто сейчас | Что нужно |
|---|---|---|---|
| R1 | **SEC-06**: macOS-биометрия хранит vault key в Keychain как generic password без `SecAccessControl`; `read()` работает без участия пользователя (тест `stores_and_reads_key_without_a_biometric_prompt` это фиксирует) | Требует прямой работы с `SecItemAdd/SecItemUpdate` через `security-framework-sys` + миграции существующих элементов; `security_framework::passwords` access control не умеет | Реализовать `SecAccessControl(BiometryCurrentSet | Or | DevicePasscode, WhenPasscodeSetThisDeviceOnly)`, миграция + ручная проверка «чтение без биометрии невозможно» |
| R2 | **SEC-07**: `VaultData`/`Entry`/`SshKey`/`UnlockSecretDto` не зануляются | `Zeroize` на моделях затрагивает serde-путь и все конструкторы; JS-куча всё равно не зануляется, поэтому выигрыш частичный | `#[derive(Zeroize, ZeroizeOnDrop)]` на моделях + `Zeroizing<String>` в DTO; в документации зафиксировать остаточный риск JS |
| R3 | **SEC-08**: LAN-передача — 6 цифр, без TTL/lockout, plaintext-HTTP, код в открытом заголовке | Меняет UX сопряжения (код длиннее, срок жизни, экран ошибок), нужен отдельный дизайн + тесты | 8 символов base32 (~40 бит), TTL 120 с, 3 попытки, AEAD архива ключом из кода |
| R4 | **SEC-09**: нет серверного счётчика неудачных разблокировок (для биометрии — только в UI) | Нужен персистентный счётчик с монотонным временем и UX «подождите» | `unlock-state.json` + экспоненциальная задержка, лимит на `biometric_unlock` |
| R5 | **SEC-11**: favicon по умолчанию включён → домены уходят в Google; кэш в `localStorage` | Продуктовое решение (нужен дефолт + предупреждение), затрагивает настройки | Дефолт «выкл», явное согласие, кэш в IndexedDB и очистка при lock |
| R6 | **SEC-15/17/18/20**: `frame-src blob:` в CSP без «защиты на будущее»; список KDF-слотов не входит в AAD payload; нет «concealed»-маркеров буфера обмена; нет фаззинга парсеров | Низкий риск/большой объём | По одному пункту: дайджест слотов в AAD; маркеры буфера; `cargo fuzz` на 7 парсеров |
| R7 | Диалог выбора папки недоступен из «Продолжить с недавним» после удаления `pms.vault` из папки — грант не восстанавливается, нужно выбрать папку заново | Осознанный компромисс: помним только папки, в которых лежит vault | Сообщение в UI «папка больше не выглядит как хранилище — выберите её снова» |

Отдельно: **границы доверия после SEC-01**. Компромисс renderer'а теперь даёт максимум «прочитать/изменить выбранные пользователем папки хранилищ» и «расшифровать данные, пока хранилище открыто» (последнее было верно и раньше — plaintext живёт в JS-сторе). Эскалация к `~/.ssh`/`LaunchAgents` требует либо нативного диалога с участием пользователя, либо подделки `fs-scope.json`, что теперь бесполезно без гранта, покрывающего сам файл.

---

## 6. Поведенческие проверки (выполнены)

| Проверка | Команда/тест | Результат |
|---|---|---|
| Грант `/`, `$HOME`, покрывающий store | `rejects_grant_store_content_that_grants_everything`, `refuses_to_grant_the_filesystem_root_or_home` | отклонено |
| Сосед с общим префиксом (`/tmp/vault` vs `/tmp/vault-evil`) | `rejects_sibling_with_a_shared_prefix` | отклонено |
| Симлинк наружу | `rejects_symlinks_pointing_outside` | отклонено |
| Save-диалог для нового файла | `grants_a_save_target_that_does_not_exist_yet` | работает, грант только на цель |
| Перезапуск: помним только папки с vault | `only_vault_folders_are_remembered`, `forgets_folders_that_no_longer_hold_a_vault` | да |
| KDF-бомбы | `rejects_kdf_parameter_bombs`, `rejects_work_bomb`, замер в `tests/operations.rs` §7 | 0.00–0.01 мс, `import_corrupt` |
| XML-сущности | `unescapes_entities_in_field_values` | `Bank & Co <main>`, `p&assword`, `?a=1&b=2` |
| Крипто после рефакторинга | `payload_roundtrip_*`, `open_attachment(seal(x)) == x`, `JSON совпал` в `tests/perf_suite.rs` §3 | побайтово |
| Архив: traversal, права, целостность, откат | тесты `pms_archive` + `transfer` | зелёные |
| Зависимости | `cargo audit`, `pnpm audit --prod` | 0 уязвимостей |

---

## 6a. Что проверено в третьем раунде

| Проверка | Чем |
|---|---|
| Data-protection keychain: запись/чтение/удаление, отсутствие элемента | `protected_read_reports_a_missing_item`, `disabling_a_missing_item_is_idempotent`; полный round-trip — `#[ignore]`-тест `protected_round_trip_after_biometric_prompt` (нужен пользователь у экрана) |
| Троттлинг: бесплатные попытки, рост задержки, сброс при успехе, переживание рестарта, часы «из будущего» | 5 тестов в `commands/attempts.rs` |
| Передача: код (алфавит, регистр, дефисы, длина), HMAC над телом, неверный код | `codes_are_long_random_and_forgiving_to_type`, `the_upload_mac_binds_the_code_and_the_body`, `http_transfer_*` |
| Зануление моделей | компиляция `Zeroize/ZeroizeOnDrop` на всех моделях секретов + 192 unit-теста |
| Clipboard-маркеры | ручная проверка (значение не попадает в историю менеджера буфера) |

## 6b. Android: сборка проверена

Все правки этого раунда проверялись на desktop; отдельно прогнан `cargo check --target aarch64-linux-android --lib` (rustup-тулчейн + NDK 28.2, `CC_aarch64_linux_android=…android24-clang`). Он нашёл реальную поломку: `dialog_pick_folder` вызывал `tauri_plugin_dialog::pick_folder`, которого на mobile нет — сборка Android падала. Теперь `dialog_pick_folder` под `#[cfg(desktop)]`, а на mobile возвращает понятную ошибку (`dialog_unsupported`): на Android папку выбирает `tauri-plugin-android-fs` из JS. После правки мобильная цель собирается.

Проверяется командой:

```bash
cd src-tauri
NDK=$HOME/Library/Android/sdk/ndk/<версия>/toolchains/llvm/prebuilt/darwin-x86_64
PATH="$NDK/bin:$HOME/.rustup/toolchains/<версия>/bin:$PATH" \
CC_aarch64_linux_android="$NDK/bin/aarch64-linux-android24-clang" \
CXX_aarch64_linux_android="$NDK/bin/aarch64-linux-android24-clang++" \
AR_aarch64_linux_android="$NDK/bin/llvm-ar" \
cargo check --target aarch64-linux-android --lib
```

Запуск на устройстве (эмулятор/телефон) в этом раунде не выполнялся: пункт остаётся в списке ниже.

## 6c. Формат v2 (SEC-17) и смоук-тесты команд (3.3)

* `VAULT_FORMAT_VERSION = 2`, `VAULT_MIN_FORMAT_VERSION = 1`: при чтении принимаются обе версии, для v2 проверяется длина дайджеста и его совпадение с набором слотов.
* `header_core_aad` добавляет `"PMS-SLOTSET-v1" + digest` только с версии 2, поэтому AAD старых файлов не изменился.
* `rewrap`/`remove_slot` читают payload с диска, расшифровывают ключом сессии и запечатывают под новый заголовок (`reseal_for_header`); слот при `rewrap` оборачивается уже против нового формата.
* `src-tauri/tests/commands_smoke.rs` (9 тестов) поднимает приложение на `tauri::test::MockRuntime` и проверяет реальный IPC: регистрацию, имена аргументов camelCase, `session_not_found`, `fs_scope`, устойчивость к мусорным аргументам и соответствие списка команд вызовам фронтенда. Команды с `AppHandle` на mock-рантайме не регистрируются (макрос фиксирует `Wry`) — для них проверка идёт по исходникам, это записано в шапке файла.

## 7. Проверка после следующего изменения (чек-лист)

```bash
cd src-tauri && cargo audit && cargo deny check advisories bans sources
cd src-tauri && cargo test                                          # 179 unit-тестов
cd src-tauri && cargo test --release --test operations -- --nocapture --test-threads=1
cd src-tauri && cargo test --release --test perf_suite -- --nocapture --test-threads=1
pnpm typecheck && pnpm lint && pnpm test && npx playwright test
# вручную (не автоматизируется): импорт-приманка KDBX/XML/zip, keyfile 10 МиБ,
# 10 попыток неверного кода по LAN, запись fs-scope.json из консоли webview + перезапуск,
# adb backup на Android, чтение без биометрии на macOS
```
