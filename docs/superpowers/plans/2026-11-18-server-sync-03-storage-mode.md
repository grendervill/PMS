# PMS Server — Implementation Plan 3: режим хранения, профили и кэш

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** При запуске приложение спрашивает, где живёт сейф — в папке или на сервере, — запоминает выбор, держит серверное хранилище в локальном кэше и подключается к серверу по мастер-ключу с проверкой отпечатка.

**Architecture:** Режим и профили — маленькие модули на TypeScript с `localStorage` (без секретов). Серверный режим использует существующий `VaultStorage` третьей реализацией поверх кэша `<app_data>/remote/<profile_id>`, который Rust открывает в `fs_scope` как собственный каталог приложения. Новых сетевых путей нет: всё уже в `remote_*` командах из плана 2.

**Tech Stack:** React 19, zustand, TypeScript, Vitest, существующие компоненты `components/ui`, Rust-команда для пути кэша, Playwright для визуальной проверки.

**Spec:** `docs/superpowers/specs/2026-11-18-server-sync-design.md` (§10 клиент: режим, кэш, профили)
**Предыдущие планы:** `...-01-server.md`, `...-02-client.md`

## Global Constraints

- Секреты (мастер-ключ, токены, `auth_seed`) в `localStorage` не попадают и через профили не передаются.
- Ключи `localStorage`: `pms.storageMode.v1`, `pms.profiles.v1`.
- Профиль: `{ id, name, baseUrl, accountId, tls, lastLoginAt }`.
- Кэш одного профиля: `<app_data>/remote/<profile_id>/{pms.vault, attachments/, backups/}`; в `fs_scope` открывается **ровно** `<app_data>/remote`, не весь `app_data`.
- `folder` в серверной реализации `VaultStorage` — абсолютный путь кэша, полученный из Rust, а не собранный в JS.
- Выбор режима запоминается и применяется при следующем запуске без повторного вопроса.
- Ошибки сервера показываются текстом `ApiError.message` с кодом; HTTP не появляется без явного «разрешить http» в форме профиля.
- UI-задача: перед отчётом снять скриншоты через Playwright (см. AGENTS.md).

## Review Focus

1. Первый запуск без сохранённого режима не должен открывать последний локальный сейф молча — сначала выбор.
2. Сохранённый серверный режим без сети должен открывать кэш, а не пустой экран и не ошибку.
3. Смена режима на «Локально» не должна терять кэш серверного хранилища и не должна открывать чужой профиль.
4. Кэш-каталог другого профиля не должен считаться доступным, если профиль удалён.
5. Форма профиля не должна сохранять профиль, если проверка сервера не прошла.

---

### Task 1: Модель режима и профилей

**Files:**
- Create: `src/features/storage/mode.ts`, `src/features/storage/profiles.ts`, `src/features/storage/mode.test.ts`, `src/features/storage/profiles.test.ts`

**Interfaces:**
- Produces: `StorageMode = "local" | "remote"`; `getMode(): StorageMode | null`; `setMode(mode)`; `clearMode()`; `ServerProfile { id, name, baseUrl, accountId, tls, lastLoginAt }`; `listProfiles(): ServerProfile[]`; `saveProfile(profile): ServerProfile[]`; `removeProfile(id)`; `activeProfile(): ServerProfile | null`; `setActiveProfile(id: string | null)`.

- [ ] **Step 1: Тесты** — режим по умолчанию `null`, сохраняется и читается; мусор в `localStorage` не ломает чтение; профиль перезаписывается по `id`, сортировка по `lastLoginAt`, удаление активного сбрасывает активный.
- [ ] **Step 2: FAIL** → `npx vitest run src/features/storage` — модулей нет.
- [ ] **Step 3: Реализовать** чтение/запись с валидацией полей (как `recent.ts`: `isProfile`) и версионированными ключами.
- [ ] **Step 4: PASS + commit.**

### Task 2: Каталог кэша в Rust и узкий грант `fs_scope`

**Files:**
- Modify: `src-tauri/src/commands/remote.rs`, `src-tauri/src/lib.rs` (оба списка команд), `src-tauri/src/lib.rs` (setup: `fs_scope.attach`)
- Create: `src-tauri/tests/remote_cache.rs`

**Interfaces:**
- Produces: `remote_cache_dir(profile_id: String) -> ApiResult<String>` — создаёт `<app_data>/remote/<profile_id>/{attachments,backups}` и возвращает абсолютный путь; `remote_cache_clear(profile_id: String) -> ApiResult<()>`.

- [ ] **Step 1: Тест** — каталог создаётся с подпапками; `profile_id` с `..`/слэшами отвергается (`bad_profile_id`); повторный вызов возвращает тот же путь.
- [ ] **Step 2: FAIL** → реализовать → **PASS**.
- [ ] **Step 3: Тест гранта**: `fs_scope.resolve(cachePath/pms.vault, Write)` после `attach` с `app_data/remote` проходит, а `app_data/fs-scope.json` — нет.
- [ ] **Step 4: commit.**

### Task 3: `remoteStorage` — третья реализация `VaultStorage`

**Files:**
- Create: `src/lib/storage/remote.ts`, `src/lib/storage/remote.test.ts`
- Modify: `src/lib/storage/index.ts`

**Interfaces:**
- Produces: `remoteStorage: VaultStorage`, где `folder` — путь кэша; все методы делегируют `desktopStorage` (или Android-реализации), `pickFolder` бросает `not_needed`; `getStorage()` больше не используется напрямую в новых местах — вместо него `storageFor(mode)`.

- [ ] **Step 1: Тест** — `remoteStorage.readVault(dir)` вызывает `storage_read_file` с `dir/pms.vault`; `writeVault` пишет туда же атомарно; `listBackups` смотрит `dir/backups`.
- [ ] **Step 2: FAIL** → реализовать тонкой обёрткой → **PASS** → commit.

### Task 4: Экран выбора режима

**Files:**
- Create: `src/features/storage/StorageModeScreen.tsx`, `StorageModeScreen.test.tsx`

**Interfaces:**
- Produces: `<StorageModeScreen onLocal={...} onServer={...} />`; ссылки на экран сервера обрабатывает родитель.

- [ ] **Step 1: Тест** — два варианта, кнопки «Работать с папкой» и «Подключить сервер» вызывают свои колбэки; клавиатурный фокус виден.
- [ ] **Step 2: FAIL** → реализовать в визуальном языке `WelcomeScreen` (две равные колонки, никаких карточек-плиток внутри карточек) → **PASS** → commit.

### Task 5: Форма профиля сервера

**Files:**
- Create: `src/features/storage/ServerProfileForm.tsx`, `ServerProfileForm.test.tsx`
- Modify: `src/features/vault/api.ts` (обёртки `remote_*`)

**Interfaces:**
- Produces: `vaultApi.remoteProbe(profile)`, `remoteFingerprint(baseUrl)`, `remoteLogin(profile, secret)`, `remoteRegister(profile, secret, vaultFolder?)`, `remoteCacheDir(profileId)`, `remoteStatus()`, `remoteLogout(accountId)`, `remotePullVault(accountId, folder)`, `remoteUsage(accountId)`.
- Produces: `<ServerProfileForm onConnected(profile) onCancel />`.

- [ ] **Step 1: Тест** — «Проверить» вызывает `remote_probe` и показывает отпечаток; без успешной проверки кнопка «Войти» заблокирована; ошибка `remote_unreachable` показывается текстом; галка «разрешить http» переключает политику на `insecure`.
- [ ] **Step 2: FAIL** → реализовать → **PASS** → commit.

### Task 6: Запуск по сохранённому режиму

**Files:**
- Modify: `src/app/App.tsx`, `src/features/vault/store.ts`, `src/features/vault/WelcomeScreen.tsx`, `src/features/storage/…`
- Create: `src/features/storage/startup.ts`, `startup.test.ts`

**Interfaces:**
- Produces: `startupTarget(): { kind: "choose" } | { kind: "local", path } | { kind: "remote", profile, cacheDir }`; стор получает `mode`, `profile`, `setMode`, `connectRemote`.

- [ ] **Step 1: Тесты** — нет режима → `choose`; режим `local` + недавний сейф → `local`; режим `remote` + профиль → `remote` с путём кэша; режим `remote` без профиля → `choose`.
- [ ] **Step 2: FAIL** → реализовать → **PASS**.
- [ ] **Step 3: Экран локального режима** получает ссылку «Сменить хранилище» → сброс режима и возврат к выбору.
- [ ] **Step 4: commit.**

### Task 7: Визуальная проверка и e2e

**Files:**
- Create: `tests/e2e/storage-mode.spec.ts`
- Modify: `tests/e2e/mock-tauri.ts` (мок `remote_*` и `remote_cache_dir`)

- [ ] **Step 1: e2e** — выбор сервера запоминается (`localStorage`), после перезагрузки страницы приложение сразу в серверном режиме; «Сменить хранилище» возвращает выбор.
- [ ] **Step 2: Скриншоты** через `webapp-testing` (Playwright): экран выбора, форма профиля с отпечатком, экран разблокировки в серверном режиме. Открыть и посмотреть глазами, исправить вёрстку.
- [ ] **Step 3: `pnpm typecheck && pnpm test && cargo test --workspace`** → commit.

## Self-Review

Синхронизация (pull/push по событиям, конфликты, офлайн-очередь) и раздел квоты в настройках — план 4. Android-грант кэша и биометрический блок версии 2 — план 5. Список блобов и GC — план 4.

**Порядок важен:** Task 1 и 2 независимы, Task 3 зависит от 2, Task 5 — от 2 и 3, Task 6 — от 1, 4, 5. Task 7 — последний.
