# macOS Biometric Unlock Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Добавить на macOS разблокировку хранилища по Touch ID с настройкой авторазблокировки и пятью попытками.

**Architecture:** Vault key (32 байта) хранится в macOS Keychain в generic-password item с `BIOMETRY_CURRENT_SET`; чтение элемента запускает системный Touch ID. Rust расшифровывает payload напрямую по ключу. React управляет 5 попытками, откатом к паролю и локальной настройкой.

**Tech Stack:** Rust, `security-framework`, `objc2-local-authentication`, Tauri 2, React 19.

**Spec:** `docs/superpowers/specs/2026-10-21-biometrics-qr-import-design.md` (§5)

## Global Constraints

- Ключ хранилища не покидает Rust и не пишется в localStorage/логи.
- Keychain item: service `dev.pms.vault.biometric`, account = `vaultId`, access control `BIOMETRY_CURRENT_SET`, protection `AccessibleWhenPasscodeSetThisDeviceOnly`.
- 5 попыток; после каждой неудачи показываем остаток; отмена/5-я неудача/системный lockout → обычный пароль.
- Авто-запрос только при запуске, не при каждом ручном lock.
- Настройка локальная для устройства; на Mac без датчика — disabled с объяснением.
- Android не трогаем в этом плане.
- Ошибки не содержат секретов; пароль-фраза и ключ не логируются.

## Review Focus

- Нет Touch ID: чекбокс disabled, приложение не падает, пароль работает.
- Keychain item пропал/набор отпечатков изменён: понятная ошибка, откат к паролю, данные целы.
- Пользователь отменил системный диалог: сразу пароль, без лишних запросов.
- Пять неудач: шестой запрос не появляется.
- Авто-запрос не повторяется при ре-рендерах экрана.

---

### Task 1: Unlock with raw vault key

**Files:**
- Modify: `src-tauri/src/core/vault.rs`
- Test: `src-tauri/src/core/vault.rs` tests

**Interfaces:**
- Produces: `pub fn unlock_vault_with_key(file_bytes: &[u8], vault_key: &[u8; 32]) -> Result<(VaultSession, VaultData), VaultError>`.
- `unlock_vault` keeps current behavior and delegates payload opening to the same helper.

- [x] **Step 1: Write failing tests**

```rust
#[test]
fn unlock_with_raw_key_roundtrips() {
    let (file, session, data) = create_vault(&config("Bio"), &passphrase("correct horse")).unwrap();
    let key = *session.vault_key();
    let (_, unlocked) = unlock_vault_with_key(&file, &key).unwrap();
    assert_eq!(unlocked, data);
}

#[test]
fn wrong_raw_key_is_rejected() {
    let (file, _, _) = create_vault(&config("Bio"), &passphrase("correct horse")).unwrap();
    assert!(matches!(
        unlock_vault_with_key(&file, &[0u8; 32]),
        Err(VaultError::BadPayload)
    ));
}
```

- [x] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml unlock_with_raw_key`
Expected: FAIL because function does not exist.

- [x] **Step 3: Implement**

Refactor `unlock_vault` so the payload-opening block becomes `unlock_vault_with_key`. Parse header, call `open_payload` with supplied key and header core AAD, deserialize, validate `data.id`, create `VaultSession`.

- [x] **Step 4: Run tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml unlock_with_raw_key`
Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add src-tauri/src/core/vault.rs
git commit -m "feat: unlock vault with raw key"
```

### Task 2: macOS Keychain biometric store

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/commands/biometric.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/commands/biometric.rs`

**Interfaces:**
- Produces commands:
  - `biometric_status() -> BiometricStatus { available: bool, biometryType: Option<String> }`.
  - `biometric_enroll(sessionId: Uuid) -> ()`.
  - `biometric_disable(vaultId: Uuid) -> ()`.
  - `biometric_unlock(fileB64: String, vaultId: Uuid, state) -> UnlockedVault { sessionId, data }`.
- Errors: `biometric_unavailable`, `biometric_failed`, `biometric_cancelled`, `biometric_lockout`, `biometric_key_missing`.

- [x] **Step 1: Add macOS dependencies**

```toml
[target.'cfg(target_os = "macos")'.dependencies]
security-framework = "3.7"
objc2-local-authentication = { version = "0.3.2", features = ["LAContext"] }
```

- [x] **Step 2: Write failing tests**

Test error mapping helper with synthetic `security_framework::base::Error` codes (extract mapping into `fn map_keychain_error(code: i32) -> ApiError`); assert user-cancel and auth-failed map to distinct codes. Do not test real Touch ID in CI.

- [x] **Step 3: Implement keychain module**

On macOS:
- `available()` uses `LAContext::new().canEvaluatePolicy_error(LAPolicy::DeviceOwnerAuthenticationWithBiometrics).is_ok()`.
- `enroll(vault_id, key)`: create `SecAccessControl::create_with_protection(Some(AccessibleWhenPasscodeSetThisDeviceOnly), AccessControlOptions::BIOMETRY_CURRENT_SET.bits())`; `PasswordOptions::new_generic_password(SERVICE, &vault_id.to_string())`; `set_access_control`; `set_generic_password_options`; then `generic_password` once to force biometric confirmation; on failure delete item.
- `disable(vault_id)`: `delete_generic_password`; ignore item-not-found.
- `read(vault_id)`: `generic_password` → zeroize buffer; require 32 bytes.
- `biometric_unlock`: decode file, read key, `unlock_vault_with_key`, put session, return.
- On non-macOS return `biometric_unavailable`.

- [x] **Step 4: Register commands**

Add to `commands/mod.rs` and `lib.rs` invoke list.

- [x] **Step 5: Run tests and clippy**

Run:
```bash
cargo test --manifest-path src-tauri/Cargo.toml biometric
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```
Expected: PASS; no warnings.

- [x] **Step 6: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/commands src-tauri/src/lib.rs
git commit -m "feat: add macOS biometric keychain"
```

### Task 3: Frontend biometric API and store

**Files:**
- Modify: `src/features/vault/api.ts`
- Create: `src/features/vault/biometric.ts`
- Modify: `src/features/vault/store.ts`
- Test: `src/features/vault/biometric.test.ts`
- Test: `src/features/vault/store.test.ts`

**Interfaces:**
- `BiometricStatus { available: boolean; biometryType?: string | null }`.
- `vaultApi.biometricStatus()`, `biometricEnroll(sessionId)`, `biometricDisable(vaultId)`, `biometricUnlock(file, vaultId)`.
- `isBiometricEnabled(vaultId)`, `setBiometricEnabled(vaultId, enabled)`.
- Store: `unlockBiometric(): Promise<{ ok: boolean; code?: string }>`; `enrollBiometric(): Promise<boolean>`; `disableBiometric(): Promise<boolean>`.
- `unlockBiometric` reads file, calls API, sets unlocked state; on failure leaves status `locked` and returns code.

- [x] **Step 1: Write failing tests**

Test local flag storage roundtrip and store unlock success/failure with mocked API/storage. Assert failure keeps `status: "locked"` and returns `biometric_failed`.

- [x] **Step 2: Run tests to verify they fail**

Run: `pnpm vitest run src/features/vault/biometric.test.ts src/features/vault/store.test.ts`
Expected: FAIL.

- [x] **Step 3: Implement API, local flag, store actions**

Use `localStorage` key `pms.biometric.<vaultId>`. `unlockBiometric` uses `getStorage().readVault(path)` and `vaultApi.biometricUnlock`. `enrollBiometric` uses current `sessionId`; `disableBiometric` uses `data.id`.

- [x] **Step 4: Run tests**

Run: `pnpm vitest run src/features/vault/biometric.test.ts src/features/vault/store.test.ts`
Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add src/features/vault
git commit -m "feat: wire biometric unlock into the vault store"
```

### Task 4: Settings and unlock screen

**Files:**
- Create: `src/features/settings/SettingsView.tsx`
- Modify: `src/app/AppShell.tsx`
- Modify: `src/features/vault/UnlockScreen.tsx`
- Test: `src/features/settings/SettingsView.test.tsx`
- Test: `src/features/vault/UnlockScreen.test.tsx`

**Interfaces:**
- `SettingsView` reads `biometricStatus`, local enabled flag, `sessionId`, `data.id`.
- Checkbox label exactly «Всегда разблокировать при помощи биометрии».
- UnlockScreen state: `biometricAvailable`, `biometricEnabled`, `attemptsLeft` (max 5), `biometricBusy`, `biometricNotice`.
- Auto attempt only once per mount when enabled+available; retries only via «Повторить».

- [x] **Step 1: Write failing tests**

Settings:
- unavailable → checkbox disabled + explanation.
- available+checked → `enrollBiometric` called; unchecking → `disableBiometric`.
UnlockScreen:
- enabled+available auto calls unlock once.
- failure shows «Осталось 4 попытки» and retry; fifth failure hides biometric and shows password.
- cancel shows password immediately.
Mock store state and API.

- [x] **Step 2: Run tests to verify they fail**

Run: `pnpm vitest run src/features/settings/SettingsView.test.tsx src/features/vault/UnlockScreen.test.tsx`
Expected: FAIL.

- [x] **Step 3: Implement SettingsView**

Replace settings EmptyState in `AppShell` with `<SettingsView />`. Include a Security card with the checkbox, status text, and an Import/Export placeholder card for later plans. Use existing `Toggle`/`Field`/`Button` styles. On enroll failure show `errorText`.

- [x] **Step 4: Implement UnlockScreen biometric flow**

- On mount, call `vaultApi.biometricStatus()` once.
- If `isBiometricEnabled(vaultId)` and available, auto attempt once.
- `attemptBiometric`: call store `unlockBiometric`; if ok no UI; if `biometric_cancelled`/`biometric_lockout`/`biometric_unavailable` → show password; if `biometric_failed` decrement and show notice; at 0 show password.
- Add «Войти по отпечатку» button when enabled+available+attemptsLeft>0.
- Keep password/keyfile UI fully functional.

- [x] **Step 5: Run tests**

Run: `pnpm vitest run src/features/settings/SettingsView.test.tsx src/features/vault/UnlockScreen.test.tsx`
Expected: PASS.

- [x] **Step 6: Commit**

```bash
git add src/features/settings src/features/vault/UnlockScreen.tsx src/app/AppShell.tsx
git commit -m "feat: add biometric settings and unlock flow"
```

### Task 5: E2E, build, visual verification

**Files:**
- Modify: `tests/e2e/mock-tauri.ts`
- Modify: `tests/e2e/vault.spec.ts`
- Modify: `README.md`

- [x] **Step 1: Add mock commands**

Mock `biometric_status` based on a page-level flag; `biometric_enroll`, `biometric_disable`, `biometric_unlock` success/failure sequences. `biometric_unlock` returns the stored vault data on success.

- [x] **Step 2: E2E scenarios**

- Settings checkbox unavailable state.
- Enable biometrics with mock available, lock, auto-unlock, fallback via 5 failures, cancel fallback.
- Assert password form remains usable.

- [x] **Step 3: Run checks**

```bash
pnpm typecheck
pnpm test
pnpm lint
pnpm test:e2e
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
pnpm tauri build
```
Expected: all pass; `.app`/`.dmg`.

- [x] **Step 4: Visual check**

With the E2E mock, screenshot Settings and UnlockScreen biometric states; verify disabled explanation, retry copy, and password form. On the native app, verify Settings shows Touch ID unavailable without crashing. Remove temp screenshots.

- [x] **Step 5: Commit**

```bash
git add tests/e2e README.md
git commit -m "test: cover biometric unlock flows"
```
