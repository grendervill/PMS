# PMS — план 1: фундамент, хранилище, записи, генераторы

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Собрать рабочий десктопный срез PMS: создание/разблокировка зашифрованного хранилища, CRUD записей, генераторы логинов и паролей, проверка силы/дублей/истории, копирование и авто-блокировка.

**Architecture:** Tauri 2 + React 19 + TypeScript + Vite + Tailwind 4. Rust-ядро хранит ключ разблокированной сессии и выполняет Argon2id/AEAD; React получает только расшифрованный `VaultData`. Файлы хранилища пишутся атомарно через Rust-команды на десктопе; Android SAF добавляется отдельным планом.

**Tech Stack:** Tauri 2.12, React 19.3, TypeScript 5.9, Vite 8, Tailwind 4.3, Radix UI, Motion, Zustand, Lucide, zxcvbn-ts, Rust 1.90+, `argon2`, `chacha20poly1305`, `aes-gcm`, `hkdf`, `sha2`, `zeroize`, `ssh-key`.

**Spec:** `docs/superpowers/specs/2026-10-20-pms-design.md`

## Global Constraints

- Платформа первого среза: macOS Apple Silicon; архитектура не должна мешать Windows/Linux/Android.
- Весь UI на русском; строки в `src/lib/i18n/ru.ts`, без хардкода в компонентах.
- Тема по умолчанию — тёмный «сейф»; светлая тема заложена токенами, но не обязательна в этом плане.
- Никакой телеметрии и сетевых запросов в этом плане; favicon/SSH/TOTP/вложения — следующие планы.
- Пароль-фраза: UTF-8 NFC, минимум 8 символов; слабая — предупреждение, не блокировка.
- Argon2id: стандарт 64 МиБ/t=3/p=4, усиленный 256 МиБ/t=4/p=4, параноидальный 512 МиБ/t=6/p=4. Параметры из заголовка не перезатираются.
- Cipher: XChaCha20-Poly1305 по умолчанию; AES-256-GCM опционально.
- Один активный vault; ключ сессии только в Rust, `zeroize` при lock.
- Записи: имя обязательно; id UUID v4 скрыт; слабые/дублирующиеся пароли сохраняются с предупреждением; старый пароль этой записи повторно использовать нельзя.
- История: 50 значений на поле.
- Авто-lock 5 минут, буфер 30 секунд; настройки меняются в UI.
- Все секреты — только в зашифрованном `VaultData`; в логах и ошибках секретов нет.
- TDD: каждый Rust-модуль и каждая бизнес-функция фронтенда получают тест до реализации.
- Каждый task заканчивается `cargo test`, `cargo clippy -- -D warnings`, `pnpm typecheck`, `pnpm test` (где применимо) и коммитом.
- Коммиты маленькие, сообщения в стиле `feat:`, `test:`, `chore:`.

## Review Focus

- **Битый/чужой файл хранилища:** приложение не падает, не логирует секреты и показывает понятную ошибку. Тест — в Task 3/5.
- **Смена пароля:** старый payload остаётся валидным, старый пароль перестаёт работать, новый работает. Тест — в Task 5.
- **Ключ-файл SSH:** OpenSSH и PEM/PKCS#8 с тем же ключом дают одинаковый секрет; смена парольной фразы ключа не ломает вход. Тест — в Task 5.
- **Авто-блокировка с грязной формой:** правки не теряются, после входа предлагается черновик. Тест — в Task 11.
- **Буфер обмена:** очищается только если пользователь не скопировал что-то новое. Тест — в Task 11.
- **Слабый/дублирующийся пароль:** предупреждение, но сохранение проходит; старый пароль записи блокируется. Тест — в Task 9/10.
- **Атомарная запись:** сбой на временном файле не портит существующий vault. Тест — в Task 7.
- **KDF-параметры из файла:** огромные значения отклоняются до выделения памяти. Тест — в Task 3.
- **Unicode-пароль:** NFC-нормализация даёт одинаковый вход на macOS/Windows/Android. Тест — в Task 5.

---

### Task 1: Каркас Tauri + React + инструменты

**Files:**
- Create: `package.json`, `pnpm-workspace.yaml` (не нужен, если один пакет), `tsconfig.json`, `tsconfig.node.json`, `vite.config.ts`, `index.html`, `.gitignore`, `README.md`
- Create: `src/main.tsx`, `src/app/App.tsx`, `src/styles/global.css`
- Create: `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`
- Test: `src/app/App.test.tsx`

**Interfaces:**
- Produces: рабочая команда `pnpm tauri dev`; `App` экспортируется по умолчанию; Rust-библиотека `pms_lib::run()`.
- Produces: npm-скрипты `dev`, `build`, `typecheck`, `test`, `test:e2e`, `lint`, `format`.

- [ ] **Step 1: Создать конфигурацию фронтенда**

`package.json` (версии зафиксировать lockfile после `pnpm install`):

```json
{
  "name": "pms",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build",
    "preview": "vite preview",
    "tauri": "tauri",
    "typecheck": "tsc --noEmit",
    "test": "vitest run",
    "test:watch": "vitest",
    "test:e2e": "playwright test",
    "lint": "eslint .",
    "format": "prettier --write ."
  },
  "dependencies": {
    "@radix-ui/react-dialog": "^1.3.1",
    "@radix-ui/react-popover": "^1.1.15",
    "@radix-ui/react-switch": "^1.2.6",
    "@radix-ui/react-tooltip": "^1.2.8",
    "@tauri-apps/api": "^2.12.4",
    "@tauri-apps/plugin-clipboard-manager": "^2.5.0",
    "@tauri-apps/plugin-dialog": "^2.8.3",
    "@tauri-apps/plugin-opener": "^2.7.2",
    "@zxcvbn-ts/core": "^4.2.0",
    "@zxcvbn-ts/language-common": "^4.1.3",
    "@zxcvbn-ts/language-ru": "^4.1.0",
    "clsx": "^2.1.1",
    "lucide-react": "^1.60.0",
    "motion": "^14.4.0",
    "react": "^19.3.1",
    "react-dom": "^19.3.1",
    "tailwind-merge": "^3.3.1",
    "zustand": "^5.0.15"
  },
  "devDependencies": {
    "@playwright/test": "^1.64.1",
    "@tailwindcss/vite": "^4.3.3",
    "@testing-library/jest-dom": "^6.9.1",
    "@testing-library/react": "^16.3.0",
    "@types/react": "^19.2.0",
    "@types/react-dom": "^19.2.0",
    "@vitejs/plugin-react": "^5.0.4",
    "eslint": "^9.39.0",
    "eslint-plugin-react-hooks": "^7.0.0",
    "jsdom": "^27.0.0",
    "prettier": "^3.6.0",
    "tailwindcss": "^4.3.3",
    "typescript": "^5.9.3",
    "vite": "^8.3.4",
    "vitest": "^5.0.5"
  }
}
```

- [ ] **Step 2: Создать Vite/TS/Tailwind конфиги и точку входа**

`vite.config.ts` использует `react()` и `tailwindcss()`, `clearScreen: false`, `server.port: 1420`, `strictPort: true`, `envPrefix: ["VITE_", "TAURI_ENV_*"]`.
`tsconfig.json` — strict, `noUncheckedIndexedAccess`, `paths: { "@/*": ["./src/*"] }`.
`src/styles/global.css` — `@import "tailwindcss";` + пустые токены.
`src/app/App.tsx` — заглушка с `<main>PMS</main>`.
`src/main.tsx` — `createRoot(...).render(<App />)`.
`index.html` — `<div id="root">`, `lang="ru"`, title PMS.

- [ ] **Step 3: Создать Tauri-конфигурацию и Rust-крейт**

`src-tauri/Cargo.toml`:

```toml
[package]
name = "pms"
version = "0.1.0"
edition = "2021"
rust-version = "1.90"

[lib]
name = "pms_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2.12", features = [] }
tauri-plugin-dialog = "2"
tauri-plugin-clipboard-manager = "2"
tauri-plugin-opener = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

`tauri.conf.json`: `productName: "PMS"`, `identifier: "dev.pms.vault"`, `build.beforeDevCommand: "pnpm dev"`, `devUrl: "http://localhost:1420"`, `beforeBuildCommand: "pnpm build"`, `frontendDist: "../dist"`, окно 1200×800, минимум 900×600, `title: "PMS"`, `theme: "Dark"`.
`capabilities/default.json`: `core:default`, `dialog:allow-open`, `dialog:allow-save`, `clipboard-manager:allow-write-text`, `clipboard-manager:allow-read-text`, `opener:allow-open-url`.
`lib.rs`: `pub fn run() { tauri::Builder::default().plugin(...).run(tauri::generate_context!()).expect(...) }`.
`main.rs`: `fn main() { pms_lib::run(); }`.

- [ ] **Step 4: Добавить тестовый каркас**

`src/app/App.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import App from "./App";

describe("App", () => {
  it("renders the product name", () => {
    render(<App />);
    expect(screen.getByText("PMS")).toBeInTheDocument();
  });
});
```

`vitest.config.ts`: environment `jsdom`, setup `src/test/setup.ts` с `@testing-library/jest-dom/vitest`.

- [ ] **Step 5: Установить и проверить**

Run: `pnpm install`
Run: `pnpm typecheck`
Expected: PASS.
Run: `pnpm test`
Expected: PASS (1 тест).
Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: PASS.
Run: `pnpm tauri dev` (вручную, затем закрыть)
Expected: окно PMS с текстом PMS.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "chore: scaffold Tauri 2 + React + TypeScript"
```

---

### Task 2: Дизайн-токены, UI-примитивы, каркас приложения

**Files:**
- Create: `src/styles/tokens.css`, `src/styles/global.css`
- Create: `src/lib/cn.ts`, `src/components/ui/Button.tsx`, `IconButton.tsx`, `Input.tsx`, `Textarea.tsx`, `Field.tsx`, `Modal.tsx`, `Toast.tsx`, `EmptyState.tsx`, `Badge.tsx`
- Create: `src/features/vault/VaultSidebar.tsx`, `src/app/AppShell.tsx`
- Test: `src/components/ui/Button.test.tsx`, `src/components/ui/Modal.test.tsx`

**Interfaces:**
- Produces: `cn(...classes)`; `Button({variant?: "primary"|"secondary"|"ghost"|"danger", size?: "sm"|"md", ...})`; `Modal({open, onOpenChange, title, children})`; `ToastProvider`/`useToast()`.
- Produces: CSS-токены `--bg`, `--surface`, `--surface-2`, `--border`, `--text`, `--muted`, `--accent`, `--accent-strong`, `--warning`, `--danger`, `--radius-*`, `--motion-*`.

- [ ] **Step 1: Загрузить skill `frontend-design`** (обязательно перед UI-кодом по AGENTS.md) и зафиксировать палитру: графит `#121614`, поверхность `#191F1C`, граница `#2B3530`, текст `#E9EEEB`, muted `#98A69E`, акцент `#43A47C`, warning `#D8A548`, danger `#D96464`.
- [ ] **Step 2: Написать тесты Button/Modal**

Button: рендерит текст, вызывает `onClick`, disabled не вызывает, variant добавляет класс.
Modal: открыт — виден заголовок и children; закрыт — не виден; Esc вызывает `onOpenChange(false)`.

- [ ] **Step 3: Запустить тесты, убедиться, что падают** (`pnpm test` — модули не найдены).
- [ ] **Step 4: Реализовать токены и примитивы** на Radix Dialog/Tooltip, Lucide, Motion; поддержать `prefers-reduced-motion`.
- [ ] **Step 5: Реализовать `AppShell`**: левая панель 240 px, логотип-монограмма, навигация (Записи, Генератор, Проверка, Настройки), низ — статус хранилища. На узком экране — нижняя навигация. Пока пункты ведут на заглушки.
- [ ] **Step 6: Проверить** `pnpm typecheck`, `pnpm test`, `pnpm tauri dev` + скриншот окна через `computer-use` (по AGENTS.md).
- [ ] **Step 7: Commit** `git commit -m "feat: add design system and app shell"`.

---

### Task 3: Rust-модели и формат vault-файла

**Files:**
- Create: `src-tauri/src/core/mod.rs`, `error.rs`, `models.rs`, `vault_file.rs`
- Modify: `src-tauri/src/lib.rs` (объявить `mod core;`)
- Test: `src-tauri/src/core/vault_file.rs` (unit-тесты в модуле)

**Interfaces:**
- Produces: `VaultData`, `Entry`, `HistoryItem`, `VaultSettings`, `Draft`, `AttachmentMeta`, `SshKey`, `TotpConfig` (serde camelCase).
- Produces: `VaultHeader { format, vault_id, created_at, cipher, kdf_slots }`, `KdfSlot { id, kind, kdf, wrap }`, `KdfParams { algo, m_kib, t, p, salt_b64 }`.
- Produces: `VaultFile { header: VaultHeader, payload: Vec<u8> }`; `parse_vault_file(bytes: &[u8]) -> Result<VaultFile, VaultError>`; `serialize_vault_file(&VaultFile) -> Result<Vec<u8>, VaultError>`.
- Produces: `VaultError` с кодами `BadMagic`, `UnsupportedVersion`, `Truncated`, `BadHeader`, `BadParams`, `NoSlots`, `TooManySlots`, `BadPayload`.

- [ ] **Step 1: Написать падающие тесты**

Тесты:
- `roundtrip_header_and_payload` — сериализовать/распарсить, поля равны.
- `rejects_bad_magic` — `b"XXXX1..."` → `BadMagic`.
- `rejects_truncated` — обрезать 10 байт → `Truncated`.
- `rejects_absurd_kdf_params` — `m_kib = 4_000_000` → `BadParams`.
- `rejects_no_slots` / `rejects_too_many_slots` (0 и 9) → соответствующая ошибка.
- `rejects_unknown_cipher` — строка `"rot13"` → `BadHeader`.
- `rejects_trailing_garbage` — лишние байты после payload → `BadPayload` (или `Truncated`; зафиксировать одно поведение).

- [ ] **Step 2: Запустить** `cargo test --manifest-path src-tauri/Cargo.toml vault_file`
Expected: FAIL (модули не созданы).

- [ ] **Step 3: Реализовать модели и парсер**

Формат: `magic "PMSV1" (5) || header_len u32 LE || header_json || payload`.
Ограничения: `header_len ≤ 1 МиБ`, `payload ≤ 512 МиБ`, `m_kib ∈ [8192, 1048576]`, `t ∈ [1,10]`, `p ∈ [1,8]`, salt 16–64 байта, slots 1–8.
`VaultData.id` должен совпадать с `header.vault_id`; проверка в Task 5 после расшифровки.
`VaultData` serde: `#[serde(rename_all = "camelCase", default)]` на новых полях; `schemaVersion: 1`.
`Entry`: `id`, `kind: "login"`, `schemaVersion: 1`, `name`, `login`, `password`, `url`, `comment`, `totp: Option`, `attachmentIds`, `sshKeyIds`, `loginHistory`, `passwordHistory`, `createdAt`, `updatedAt`.

- [ ] **Step 4: Запустить** `cargo test`, `cargo clippy -- -D warnings` → PASS.
- [ ] **Step 5: Commit** `git commit -m "feat: add vault models and file format"`.

---

### Task 4: Крипто-ядро

**Files:**
- Create: `src-tauri/src/core/crypto.rs`
- Modify: `src-tauri/src/core/mod.rs`
- Modify: `src-tauri/Cargo.toml` (argon2, chacha20poly1305, aes-gcm, hkdf, sha2, zeroize, rand, base64, uuid, unicode-normalization, ssh-key, rsa, p256/p384/p521, thiserror)
- Test: `src-tauri/src/core/crypto.rs`

**Interfaces:**
- Produces: `KdfProfile::{Standard, Strong, Paranoid}` → `KdfParams`.
- Produces: `derive_kek(secret: &[u8], params: &KdfParams) -> Result<Zeroizing<[u8;32]>>`.
- Produces: `wrap_vault_key(kek, vault_key, slot_aad) -> WrapData`; `unwrap_vault_key(...)`.
- Produces: `seal_payload(vault_key, header_core_aad, data: &[u8]) -> Vec<u8>`; `open_payload(vault_key, header_core_aad, envelope: &[u8]) -> Result<Vec<u8>>`.
- Produces: `canonical_secret(kind: UnlockSecretInput) -> Result<Zeroizing<Vec<u8>>>`: passphrase NFC; arbitrary file raw bytes hashed; SSH key canonical material.
- Produces: `header_core_aad(&VaultHeader) -> Vec<u8>`, `slot_aad(&VaultHeader, &KdfSlot) -> Vec<u8>`.
- Produces: `seal_attachment`/`open_attachment` позже, не в этом task (только сигнатуры-заглушки не создавать).

- [ ] **Step 1: Написать падающие тесты**

- `argon2_is_deterministic` — одинаковый секрет/соль/параметры → одинаковый KEK; другой секрет → другой.
- `passphrase_nfc_normalization` — `"e\u{301}"` и `"é"` дают одинаковый секрет.
- `wrap_unwrap_roundtrip` — ключ совпадает; неверный KEK → ошибка.
- `slot_aad_binds_vault_and_slot` — изменение vaultId/slotId/kind/params → unwrap падает.
- `payload_roundtrip_xchacha` / `payload_roundtrip_aes_gcm`.
- `payload_rejects_tampered_tag` — перевернуть байт → ошибка.
- `payload_rejects_changed_header_core` — изменить createdAt/cipher → ошибка.
- `payload_salt_changes_key` — два seal одного plaintext дают разные ciphertext и разные salt.
- `ssh_keyfile_stable_across_formats` — сгенерировать Ed25519, сериализовать OpenSSH и PKCS#8 PEM, получить одинаковый секрет.
- `ssh_keyfile_passphrase_does_not_change_secret` — зашифровать OpenSSH ключ парольной фразой, расшифровать, секрет тот же.
- `arbitrary_file_secret_uses_bytes` — два разных файла → разные секреты.
- `kdf_params_are_capped` — `m_kib` 4 ГиБ → `BadParams`.

- [ ] **Step 2: Запустить тесты** → FAIL.
- [ ] **Step 3: Реализовать**

Argon2id: `Params::new(m_kib, t, p, Some(32))`, `hash_password_into(secret, salt, &mut kek)`.
Wrap: XChaCha20-Poly1305 с KEK, 24-байтовый случайный nonce.
Payload envelope: `version u8 = 1 || salt[32] || nonce[24|12] || ciphertext`.
`payload_key = HKDF-SHA256(vault_key, salt, b"PMS vault payload v1", 32)`.
AAD — `header_core_aad`.
Header core AAD: `b"PMS-CORE-v1" || format u16 LE || vault_id[16] || cipher_id u8 || len(created_at) u32 || created_at`.
Slot AAD: `b"PMS-SLOT-v1" || vault_id || slot_id || kind_id || m_kib u32 || t u32 || p u32 || salt`.
SSH canonicalization: parse OpenSSH/PEM/PKCS#8; decrypt if needed; Ed25519 seed 32 байта, RSA `to_pkcs1_der`, ECDSA scalar bytes. Domain `b"PMS-KEYFILE-SSH-v1" || type_tag || material`; arbitrary file `b"PMS-KEYFILE-FILE-v1" || bytes`. Return SHA-256 digest as secret.
`zeroize` all temporary key material; use `Zeroizing`.

- [ ] **Step 4: Запустить тесты + clippy** → PASS.
- [ ] **Step 5: Commit** `git commit -m "feat: add Argon2id vault crypto"`.

---

### Task 5: Создание, разблокировка, сохранение, смена пароля

**Files:**
- Create: `src-tauri/src/core/vault.rs`, `src-tauri/src/core/session.rs`
- Modify: `src-tauri/src/core/mod.rs`
- Test: `src-tauri/src/core/vault.rs` (unit/integration)

**Interfaces:**
- Produces: `CreateVaultConfig { name, kdf_profile, cipher, passphrase: Option<String>, keyfile: Option<KeyfileInput> }`.
- Produces: `create_vault(config) -> Result<(Vec<u8> file, VaultSession, VaultData)>`.
- Produces: `unlock_vault(file_bytes: &[u8], secret: UnlockSecretInput) -> Result<(VaultSession, VaultData)>`.
- Produces: `save_vault(session: &VaultSession, data: &VaultData) -> Result<Vec<u8>>`.
- Produces: `rewrap_vault(session: &VaultSession, new_secret: UnlockSecretInput, keep_existing: bool) -> Result<Vec<u8>>`.
- Produces: `SessionManager` с `unlock`, `save`, `rewrap`, `lock`, `take`.
- Produces: `VaultSession { id: Uuid, vault_key: Zeroizing<[u8;32]>, header: VaultHeader }`; `Drop` затирает ключ.

- [ ] **Step 1: Написать падающие тесты**

- `create_then_unlock_returns_same_data`.
- `unlock_with_wrong_passphrase_fails`.
- `unlock_with_keyfile_works` (произвольный файл).
- `unlock_with_encrypted_ssh_keyfile_works` (плюс неверная парольная фраза ключа → ошибка).
- `save_updates_updated_at_and_preserves_entries` (или явно: save не меняет `updatedAt` сам, это делает UI; зафиксировать).
- `rewrap_adds_second_slot` — старый пароль и новый ключ-файл оба работают.
- `rewrap_changes_passphrase` — старый пароль не работает, новый работает, payload тот же.
- `rewrap_cannot_remove_last_slot`.
- `tampered_header_cipher_fails_unlock` (payload AAD).
- `vault_id_mismatch_rejected` — `VaultData.id` не равен header vault_id.
- `header_params_from_foreign_vault_are_used` — открыть vault с Strong-профилем и проверить, что unlock использует его параметры, а не дефолт.
- `session_lock_zeroizes_and_removes`.

- [ ] **Step 2: Запустить** → FAIL.
- [ ] **Step 3: Реализовать**

Создание: сгенерировать `vaultId` (UUID v4), `vaultKey`, KDF-соли; слоты для passphrase и/или keyfile; `VaultData { schemaVersion:1, id, name, ... empty arrays }`; payload зашифровать; файл сериализовать.
Unlock: `inspect`/parse header; попробовать каждый слот выбранного типа; Argon2; unwrap; open payload; parse JSON; проверить `data.id == header.vault_id`; вернуть сессию.
Save: сериализовать `VaultData`, зашифровать с тем же header core; вернуть полный файл.
Rewrap: пересобрать слоты, не трогая payload. `keep_existing` позволяет добавить/заменить один слот. В v1 UI вызывает add/remove/change.
SessionManager: `Mutex<Option<ActiveSession>>`, один активный; `lock` берёт и дропает.
Ошибки: не раскрывать, какой слот не подошёл; снаружи `InvalidCredentials`.

- [ ] **Step 4: Запустить тесты + clippy** → PASS.
- [ ] **Step 5: Commit** `git commit -m "feat: add vault lifecycle and sessions"`.

---

### Task 6: Tauri-команды и генераторы

**Files:**
- Create: `src-tauri/src/commands/mod.rs`, `vault.rs`, `generator.rs`, `storage.rs`
- Create: `src-tauri/src/core/generator.rs`
- Modify: `src-tauri/src/lib.rs` (manage `SessionManager`, register commands)
- Test: `src-tauri/src/core/generator.rs`; command-level tests через чистые функции.

**Interfaces:**
- Produces (invoke):
  - `vault_inspect(fileB64: string) -> VaultPublicInfo`
  - `vault_create(config: CreateVaultConfig, secret: UnlockSecretInput) -> { sessionId, fileB64, data }`
  - `vault_unlock(fileB64, secret) -> { sessionId, data }`
  - `vault_save(sessionId, data) -> { fileB64 }`
  - `vault_rewrap(sessionId, secret, keepExisting) -> { fileB64 }`
  - `vault_lock(sessionId) -> void`
  - `generate_password(options: PasswordOptions) -> string`
  - `generate_login(options: LoginOptions) -> string`
- Produces: `PasswordOptions { length, lowercase, uppercase, digits, symbols, customSymbols, excludeAmbiguous }`; `LoginOptions { length, lowercase, uppercase, digits, symbols, excludeAmbiguous }`.
- Produces: file bytes передаются как base64 (`fileB64`) на этом этапе; для вложений позже будет raw IPC.

- [ ] **Step 1: Написать падающие тесты генератора**

- `password_length_is_respected` (8, 16, 128).
- `password_uses_only_selected_classes`.
- `password_contains_each_selected_class` (для length ≥ числа классов).
- `password_excludes_ambiguous` when flag true.
- `password_rejects_length_out_of_range` (7, 129).
- `login_default_is_12_lowercase_digits`.
- `login_respects_custom_charset`.
- `two_generations_differ` (вероятностный, но с 32+ символами практически всегда).
- `custom_symbols_are_used` (задать `"!@"`, проверить, что все символы из набора).

- [ ] **Step 2: Запустить** → FAIL.
- [ ] **Step 3: Реализовать генератор**

`rand::rngs::OsRng` + `Uniform`; для пароля собрать по одному символу из каждой включённой группы, добрать остальные, перемешать Fisher–Yates. Запрещённые символы (`0O1lI|`) убирать из наборов до выбора. Если после фильтрации группа пуста — ошибка `EmptyCharset`.
Для логина — равномерный выбор из объединённого набора, без гарантий групп.
Ограничения: пароль 8–128, логин 8–64.

- [ ] **Step 4: Реализовать команды**

`vault_inspect` парсит файл и возвращает `{ vaultId, format, createdAt, cipher, slotKinds, fileHash }` (SHA-256 файла).
`vault_create` создаёт сессию и возвращает base64 файла и данные.
`vault_unlock` читает файл, возвращает данные.
`vault_save` возвращает новый файл.
`vault_rewrap` возвращает новый файл.
`vault_lock` удаляет сессию.
Все ошибки — `{ code, message }` без секретов.

- [ ] **Step 5: Запустить** `cargo test`, `cargo clippy` → PASS.
- [ ] **Step 6: Commit** `git commit -m "feat: expose vault commands and generators"`.

---

### Task 7: Desktop-хранилище и атомарная запись

**Files:**
- Create: `src-tauri/src/commands/storage.rs`
- Create: `src/lib/storage/types.ts`, `src/lib/storage/desktop.ts`, `src/lib/storage/index.ts`, `src/lib/storage/recent.ts`
- Test: `src-tauri/src/commands/storage.rs` (unit, tempdir); `src/lib/storage/recent.test.ts`

**Interfaces:**
- Produces (invoke): `storage_read_file(path) -> { b64 }`; `storage_write_atomic(path, b64, mode?: number) -> void`; `storage_exists(path) -> bool`; `storage_mkdir(path, mode?: number) -> void`; `storage_stat(path) -> { isFile, isDir, size, mtimeMs }`; `storage_remove_file(path) -> void`.
- Produces: `VaultStorage`:
  ```ts
  type VaultStorage = {
    pickFolder(): Promise<string | null>;
    readVault(folder: string): Promise<Uint8Array>;
    writeVault(folder: string, bytes: Uint8Array): Promise<void>;
    listBackups(folder: string): Promise<string[]>;
    writeBackup(folder: string, name: string, bytes: Uint8Array): Promise<void>;
    statVault(folder: string): Promise<{ size: number; mtimeMs: number }>;
  };
  ```
- Produces: `desktopStorage` реализация; `getStorage()` возвращает desktop (Android — ошибка «не поддержано в этой сборке»).
- Produces: `recentVaults` localStorage API: `list()`, `add({ path, name, vaultId, lastOpenedAt })`, `remove(path)`, `last()`.

- [ ] **Step 1: Написать падающие тесты**

Rust:
- `atomic_write_replaces_existing_file`.
- `atomic_write_failure_keeps_original` — сделать temp path недоступным/симулировать ошибку; оригинал цел.
- `unix_permissions_are_0600` (cfg unix).
- `rejects_write_outside_allowed?` — нет allowlist в v1; не тестировать.
TS:
- `recent_add_updates_existing_and_sorts`.
- `recent_remove_deletes`.
- `desktop_storage_joins_paths` (mock invoke).

- [ ] **Step 2: Запустить** → FAIL.
- [ ] **Step 3: Реализовать Rust storage**

`atomicwrites` crate или собственный temp+rename; на Windows использовать `MoveFileEx`/`atomicwrites`. Записывать в `<path>.tmp-<uuid>`, `sync_all`, rename. На Unix — `PermissionsExt::from_mode(0o600)` для vault, `0o700` для каталогов. Ошибки — `StorageError`.
Не логировать содержимое.

- [ ] **Step 4: Реализовать TS-адаптер**

`desktop.ts` использует `invoke` и `open` из dialog plugin (`directory: true`).
`recent.ts` хранит JSON в `localStorage["pms.recentVaults"]`, версия 1, не более 20 записей.

- [ ] **Step 5: Проверить** `cargo test`, `pnpm test` → PASS.
- [ ] **Step 6: Commit** `git commit -m "feat: add atomic desktop vault storage"`.

---

### Task 8: Экраны приветствия, создания, входа и блокировки

**Files:**
- Create: `src/features/vault/api.ts`, `store.ts`, `WelcomeScreen.tsx`, `CreateVaultWizard.tsx`, `UnlockScreen.tsx`, `LockedOverlay.tsx`
- Create: `src/features/vault/useAutoLock.ts` (базовая версия; черновики — Task 11)
- Modify: `src/app/App.tsx`, `src/app/AppShell.tsx`
- Test: `src/features/vault/store.test.ts`, `src/features/vault/CreateVaultWizard.test.tsx`

**Interfaces:**
- Produces: `useVaultStore`:
  ```ts
  type VaultState = {
    status: "welcome" | "unlocking" | "unlocked" | "locked";
    sessionId: string | null;
    vaultPath: string | null;
    publicInfo: VaultPublicInfo | null;
    data: VaultData | null;
    recent: RecentVault[];
    error: string | null;
    createVault(input): Promise<void>;
    openVault(path): Promise<void>;
    unlock(secret): Promise<void>;
    save(data): Promise<void>;
    lock(): Promise<void>;
    reset(): void;
  };
  ```
- Produces: `vaultApi` функции, соответствующие командам Task 6.
- Produces: `CreateVaultWizard` шаги: папка+имя → защита → подтверждение; вызывает `createVault`, пишет файл, добавляет recent.
- Produces: `UnlockScreen` показывает имя/путь, вкладки по `slotKinds`, поле пароля/выбор ключ-файла; ошибка без подсказок.

- [ ] **Step 1: Написать падающие тесты стора**

- `initial_state_is_welcome`.
- `create_vault_sets_unlocked_and_persists_file` (mock api/storage).
- `open_vault_inspects_and_shows_slots`.
- `unlock_with_wrong_secret_keeps_locked_and_sets_error`.
- `lock_clears_data_and_session`.
- `recent_is_updated`.

Тест мастера:
- имя обязательно;
- пароль < 8 не проходит;
- пароли не совпадают — ошибка;
- успешный сабмит вызывает `createVault`.

- [ ] **Step 2: Запустить** → FAIL.
- [ ] **Step 3: Реализовать api/store**

`vaultApi` конвертирует `Uint8Array` ↔ base64 (`btoa`/`atob` чанками), вызывает `invoke`.
`store` не хранит пароль после unlock; `data` в памяти; `lock` вызывает команду и очищает.
`openVault` читает файл, вызывает `vault_inspect`, сохраняет путь и publicInfo.
`createVault` выбирает папку, проверяет отсутствие `pms.vault` (storage.exists), создаёт `attachments/` и `backups/`, пишет файл, добавляет recent.

- [ ] **Step 4: Реализовать экраны**

Welcome: две карточки действий, список recent, тема.
Wizard: 3 шага, прогресс, валидация, выбор профиля KDF/cipher, предупреждение об отсутствии восстановления.
Unlock: вкладки «Пароль-фраза» / «Ключ-файл»; для ключ-файла — кнопка выбора, при SSH с парольной фразой — дополнительное поле (API принимает `sshPassphrase`).
LockedOverlay: экран входа поверх приложения; кнопка «Другое хранилище».
`useAutoLock`: 5 минут, сбрасывается на pointer/key; при срабатывании вызывает `lock` (в Task 11 добавить черновики).

- [ ] **Step 5: Проверить** `pnpm typecheck`, `pnpm test`, `pnpm tauri dev` + визуально.
- [ ] **Step 6: Commit** `git commit -m "feat: add vault onboarding and unlock flow"`.

---

### Task 9: Записи — список, просмотр, создание, редактирование, история

**Files:**
- Create: `src/features/entries/EntryList.tsx`, `EntryCard.tsx`, `EntryDetail.tsx`, `EntryEditor.tsx`, `HistoryDialog.tsx`, `entries.ts` (чистые функции)
- Modify: `src/app/AppShell.tsx`, `src/features/vault/store.ts` (entry actions)
- Test: `src/features/entries/entries.test.ts`, `EntryEditor.test.tsx`

**Interfaces:**
- Produces: `createEntry(data, draft): Entry`; `updateEntry(entry, draft, now): { entry, blockedReason?: "old-password" }`; `findDuplicatePasswords(entries, password, excludeId?) -> Entry[]`; `entryStrength(password) -> 0..4` (zxcvbn).
- Produces: store actions `addEntry(draft)`, `updateEntry(id, draft)`, `deleteEntry(id)`, `getEntry(id)`. Все вызывают `save()` и пишут файл.
- Produces: `EntryEditor` поля имя/логин/пароль/URL/комментарий; кнопки генератора; индикатор силы; предупреждение о дубле; «Сохранить»/«Отменить»; блокировка старого пароля.
- Produces: `EntryDetail` с reveal/copy логина/пароля, URL, комментарий, даты; кнопки «Редактировать», «История», «Удалить».
- Produces: `EntryList` с поиском, сортировкой, пустым состоянием, карточками (имя + комментарий, если есть).

- [ ] **Step 1: Написать падающие тесты**

- `create_entry_requires_name`.
- `create_entry_sets_uuid_and_timestamps`.
- `update_entry_pushes_old_login_and_password_to_history`.
- `update_entry_blocks_old_password` — новый пароль равен любому из `passwordHistory` → `blockedReason`.
- `update_entry_allows_current_password` (без изменений).
- `find_duplicate_passwords_returns_other_entries_only`.
- `delete_entry_removes_it`.
- `search_matches_name_login_url_comment_case_insensitive`.
- UI: пустое имя — ошибка; дубль — warning, save проходит; старый пароль — save заблокирован; cancel не меняет store.

- [ ] **Step 2: Запустить** → FAIL.
- [ ] **Step 3: Реализовать чистые функции**

История: при изменении поля и непустом старом значении — push `{ value, changedAt }`; лимит 50; не дублировать подряд.
Старый пароль: если `draft.password` есть в `passwordHistory` — блок; если равен текущему — не менять/не блокировать.
Дубли: только непустые пароли, исключая текущую запись.
`updatedAt` обновляется при сохранении; `createdAt` не меняется.

- [ ] **Step 4: Реализовать UI**

Список — сетка карточек; клик открывает детали (справа/модально). Редактор — модальное окно с фиксированным футером. Все кнопки копирования вызывают `copySecret` (Task 11; до него — заглушка с toast).
Пароль в деталях скрыт; reveal автоматически скрывается через 30 секунд.
История — модалка со списком, маска, reveal/copy; старые пароли нельзя применить.
Удаление — подтверждение с именем.

- [ ] **Step 5: Проверить** `pnpm typecheck`, `pnpm test`, визуально.
- [ ] **Step 6: Commit** `git commit -m "feat: add entry CRUD with history and duplicate checks"`.

---

### Task 10: Генераторы, сила пароля, экран проверки

**Files:**
- Create: `src/features/generator/GeneratorDialog.tsx`, `options.ts`, `strength.ts`, `src/features/audit/AuditView.tsx`
- Modify: `src/features/entries/EntryEditor.tsx` (кнопки генератора), `AppShell.tsx` (навигация)
- Test: `src/features/generator/options.test.ts`, `GeneratorDialog.test.tsx`, `src/features/audit/audit.test.ts`

**Interfaces:**
- Produces: `defaultPasswordOptions: PasswordOptions` (длина 16, lower+upper+digits+symbols, excludeAmbiguous=true).
- Produces: `defaultLoginOptions` (12, lower+digits).
- Produces: `passwordStrength(password): { score: 0..4; label: string }` на zxcvbn-ts + ru/common.
- Produces: `GeneratorDialog({ mode: "password"|"login", open, onOpenChange, onUse, initialOptions? })`; кнопка «Сгенерировать» вызывает Rust-команду, «Использовать» отдаёт значение, «Скопировать» — в буфер.
- Produces: `AuditView` — группы дублей, слабые записи, записи без TOTP (информационно); клик ведёт к записи.

- [ ] **Step 1: Написать падающие тесты**

- дефолтные опции соответствуют спецификации;
- переключение групп не оставляет пустой набор;
- длина 8–128 (пароль) и 8–64 (логин);
- strength: `"password"` → score ≤ 2, `"correct horse battery staple"` → ≥ 3 (проверить фактическое значение zxcvbn и зафиксировать);
- диалог: regenerate вызывает api, use отдаёт значение, cancel не отдаёт;
- audit: дубли группируются, слабые попадают, пустые пароли игнорируются.

- [ ] **Step 2: Запустить** → FAIL.
- [ ] **Step 3: Реализовать options/strength**

zxcvbn инициализируется лениво (`await import`) при открытии диалога/редактора; словари ru + common. Оценки 0–4, подписи: «очень слабый», «слабый», «средний», «надёжный», «очень надёжный».
Опции генератора: пароль 8–128, логин 8–64; при выключении всех групп — кнопка генерации disabled + подсказка.

- [ ] **Step 4: Реализовать диалог и audit**

Диалог: крупный моноширинный preview, слайдер/число длины, переключатели групп, сетка спецсимволов, «исключить похожие», «Сгенерировать» (пересоздаёт), «Использовать», «Скопировать». Настройки сохраняются в localStorage.
Audit: карточки/списки; ссылки на записи; пустое состояние «Всё спокойно».

- [ ] **Step 5: Проверить** `pnpm typecheck`, `pnpm test`, визуально.
- [ ] **Step 6: Commit** `git commit -m "feat: add password/login generators and audit view"`.

---

### Task 11: Буфер обмена, авто-блокировка, черновики, тосты

**Files:**
- Create: `src/lib/clipboard.ts`, `src/features/vault/drafts.ts`, `src/components/ui/ToastProvider.tsx`
- Modify: `src/features/vault/useAutoLock.ts`, `store.ts`, `EntryEditor.tsx`
- Test: `src/lib/clipboard.test.ts`, `src/features/vault/drafts.test.ts`

**Interfaces:**
- Produces: `copySecret(value, { clearAfterMs = 30_000 })`: пишет в clipboard, запоминает хэш значения, через таймер читает clipboard и очищает только если там всё ещё это значение; если пользователь скопировал другое — не трогает. Возвращает cleanup.
- Produces: `saveDraft(data, draft): VaultData`; `takeDraft(data, id): { data, draft? }`; `discardDraft(data, id): VaultData`.
- Produces: `useAutoLock({ onLock, isDirty, saveDraft })`: 5 минут idle; при срабатывании: если dirty — `saveDraft` + `lock`, иначе `lock`; на manual lock — диалог.
- Produces: `ToastProvider` + `useToast().show({ title, description?, tone })`.

- [ ] **Step 1: Написать падающие тесты**

- clipboard: записывает значение; через таймер очищает, если не изменилось; не очищает, если изменилось; cleanup отменяет таймер.
- drafts: save создаёт/обновляет; take возвращает и удаляет; discard удаляет; лимит один черновик на id.
- auto-lock: fake timers — 5 минут → onLock; активность сбрасывает; dirty → saveDraft вызван до onLock.
- toast: show добавляет, auto-dismiss, несколько.

- [ ] **Step 2: Запустить** → FAIL.
- [ ] **Step 3: Реализовать**

Clipboard plugin: `writeText`, `readText`; при ошибке чтения — просто очистить (best effort). Таймер очистки хранится в модуле; новый copy отменяет старый.
Drafts: `VaultData.drafts`; черновик содержит поля формы + `updatedAt`; при открытии редактора, если есть черновик, показать баннер «Восстановить несохранённые изменения?».
Auto-lock: idle events `pointerdown`, `keydown`, `wheel`, `touchstart`; при активном системном диалоге — пауза (флаг). При dirty — сохранить черновик и vault, затем lock. Если save упал — не блокировать, показать toast.
Manual lock: диалог с тремя кнопками.

- [ ] **Step 4: Проверить** `pnpm typecheck`, `pnpm test`, вручную: скопировать, подождать, проверить; свернуть/подождать; dirty + lock.
- [ ] **Step 5: Commit** `git commit -m "feat: add clipboard clearing, drafts, and auto-lock"`.

---

### Task 12: E2E, сборка, визуальная проверка

**Files:**
- Create: `playwright.config.ts`, `tests/e2e/vault.spec.ts`, `tests/e2e/entries.spec.ts`
- Modify: `README.md`
- Test: E2E.

**Interfaces:**
- Produces: воспроизводимый сценарий: создать vault во временной папке → добавить запись → сгенерировать пароль → сохранить слабый/дубль → проверить предупреждения → попробовать старый пароль (блок) → lock → unlock → данные на месте.

- [ ] **Step 1: Настроить Playwright** для Tauri: запускать `pnpm tauri dev` или использовать web-режим с mock storage? Для реального E2E нужен Tauri-драйвер. Если он недоступен, E2E на Vite с mock `@tauri-apps/api` + отдельные Rust-тесты. Зафиксировать выбор: **Playwright против Vite с mock API** для UI-потоков; крипто/файлы — Rust-тесты; ручная проверка Tauri.
- [ ] **Step 2: Написать сценарии** create/unlock/lock, entry CRUD, generator, duplicate/weak warnings, history block.
- [ ] **Step 3: Запустить** `pnpm test:e2e` → PASS.
- [ ] **Step 4: Собрать** `pnpm build`, `cargo test`, `cargo clippy -- -D warnings`, `pnpm tauri build` (если Xcode CLT позволяет; иначе зафиксировать ошибку и использовать `tauri dev`).
- [ ] **Step 5: Визуальная проверка** — запустить приложение, снять скриншот окна через `computer-use`, проверить: окно отрисовано, тёмная тема, навигация, список, модалки, текст не обрезан.
- [ ] **Step 6: Commit** `git commit -m "test: add e2e coverage and build docs"`.

---

## Self-Review

- Spec coverage этого плана: vault, записи, генераторы, сила/дубли/история, буфер, авто-lock, черновики, desktop. Вложения/TOTP/SSH/Android/импорт — следующие планы; это зафиксировано в шапке.
- Типы: `VaultData`/`Entry`/`VaultPublicInfo`/`UnlockSecretInput` совпадают между задачами; `saveVault` возвращает полный файл; `rewrap` возвращает полный файл.
- Review Focus покрыт тестами в Task 3/5/7/9/11.
- Пропорция: план описывает интерфейсы и проверки, не тела функций.
