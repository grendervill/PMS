# PMS — план 2: вложения и TOTP

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox syntax.

**Goal:** Добавить к записям зашифрованные вложения с превью и TOTP-коды с живым обновлением и общим экраном.

**Architecture:** Rust-ядро расширяется AEAD-форматом вложений и RFC 6238 TOTP; React получает только расшифрованные байты по запросу. Метаданные вложений лежат в `VaultData.attachments`, байты — в `attachments/<id>.pmsa`. TOTP-секрет — в записи; код считается Rust-командой.

**Tech Stack:** Rust (`chacha20poly1305`, `hkdf`, `sha2`, `hmac`, `sha1`, `data-encoding`, `url`), React, Tauri 2.

**Spec:** `docs/superpowers/specs/2026-10-20-pms-design.md`

## Global Constraints

- Вложения шифруются XChaCha20-Poly1305, AAD = `vaultId || attachmentId || version || originalSize`.
- Лимит файла: 50 МБ по умолчанию; жёсткий предел команды 100 МБ.
- В v1 файл передаётся целиком через base64 IPC; потоковые чанки — позже.
- TOTP: RFC 6238, SHA1/256/512, 6/8 цифр, период 30/60; секрет base32.
- Не логировать секреты, содержимое файлов и TOTP-секреты.
- Все новые UI-строки — по-русски.
- TDD: тест до реализации; каждый task заканчивается `pnpm typecheck && pnpm test && pnpm lint` и/или `cargo test && cargo clippy -D warnings`.
- Не менять формат существующего vault; новые поля `VaultData`/`Entry` — с `#[serde(default)]`/optional.

## Review Focus

- Битая/подменённая вложенность: открытие падает, приложение не отдаёт чужие байты.
- Вложение одного vault нельзя открыть в другом/под другим id.
- TOTP-код совпадает с RFC 6238 для SHA1/256/512.
- TOTP-секрет не уходит в логи; неверный base32/URI даёт понятную ошибку.
- Вложения не теряются при сохранении записи и переживают lock/unlock.
- UI не показывает TOTP-код записи без TOTP и корректно очищает интервалы.

---

### Task 1: Rust attachment format + commands

**Files:** `src-tauri/src/core/attachments.rs`, `src-tauri/src/commands/attachments.rs`, `core/mod.rs`, `commands/mod.rs`, `lib.rs`, `Cargo.toml`.

**Interfaces:**
- `seal_attachment(vault_key, vault_id, attachment_id, plaintext) -> Result<Vec<u8>>`
- `open_attachment(vault_key, vault_id, attachment_id, blob) -> Result<Vec<u8>>`
- `attachment_seal(session_id, name, mime, b64) -> { blobB64, meta: AttachmentMeta }`
- `attachment_open(session_id, attachmentId, blobB64) -> { b64 }`
- `AttachmentMeta` already exists; id UUID, sha256 hex, kind image/pdf/text/other, size u64.

- [ ] Tests: roundtrip, tamper, wrong attachment id, wrong vault id, oversize reject, blob magic/length checks.
- [ ] Format: `PMSA1 || version u8 || cipher u8 || salt[32] || nonce[24] || original_size u64 || ciphertext`.
- [ ] Derive key `HKDF(vault_key, salt, "PMS attachment v1")`; AAD as above; `sha256` plaintext.
- [ ] Command finds session vault id/key via `SessionManager::with`.
- [ ] Commit.

### Task 2: Attachment model/API/storage + editor/detail UI

**Files:** `src/features/attachments/*`, `src/features/vault/api.ts`, `types.ts`, `store.ts`, `entries.ts`, `EntryEditor.tsx`, `EntryDetail.tsx`, `src/lib/storage/*`.

**Interfaces:**
- `EntryDraft` adds `attachmentIds: string[]`; `Entry` create/update/entryToDraft carry it.
- `vaultApi.sealAttachment(sessionId, name, mime, bytes) -> { blob: Uint8Array; meta: AttachmentMeta }`
- `vaultApi.openAttachment(sessionId, id, blob) -> Uint8Array`
- Store: `addAttachmentMeta(meta)`, maybe `removeAttachmentMeta`.
- Storage: `saveFile(path, bytes)` for downloads.
- UI editor: add/remove attachments, show names/sizes; add writes blob immediately + metadata via store.
- UI detail: list attachments; preview images/text/PDF in modal; download; delete from entry.
- Preview: SVG not rendered; text max 1 MB; PDF iframe with download fallback.
- Commit.

### Task 3: TOTP core + commands

**Files:** `src-tauri/src/core/totp.rs`, `commands/totp.rs`, `Cargo.toml`.

**Interfaces:**
- `totp_inspect(input: String) -> TotpConfig` (base32 or otpauth URI).
- `totp_code(config: TotpConfig, at: Option<u64>) -> { code: String, remaining: u32 }`.
- Defaults: SHA1, 6 digits, 30 s; validate base32; reject empty/invalid.
- Tests: RFC vectors for SHA1/256/512, 6/8 digits, periods, URI parse.

- [ ] Implement HMAC dynamic truncation; base32 no-pad.
- [ ] Commit.

### Task 4: TOTP UI

**Files:** `src/features/totp/*`, `EntryEditor.tsx`, `EntryDetail.tsx`, `AppShell.tsx`, `navigation.ts`, `types.ts`, `entries.ts`.

**Interfaces:**
- `TotpCode({ config, compact? })` — live code, countdown, copy.
- `TOTPView` — all entries with TOTP, search optional, empty state.
- Editor: field accepts base32/otpauth; parse on blur/save; show issuer/period and preview; invalid blocks save with message.
- Detail: TOTP code block if configured.
- Add nav section `totp`; AppShell renders TOTPView.
- Commit.

### Task 5: E2E, build, visual check

- [ ] E2E: add attachment via mocked file dialog/command, see it in detail, preview; add TOTP, see live code; lock/unlock persists.
- [ ] `pnpm typecheck`, `pnpm test`, `pnpm test:e2e`, `cargo test`, `cargo clippy`, `pnpm tauri build`.
- [ ] Visual screenshot of attachment preview and TOTP screen.
- [ ] Commit.

## Self-Review

- Attachments and TOTP are independent; shared `EntryDraft` change is the only cross-task interface.
- Existing Plan 1 tests must be updated only for the new `attachmentIds`/`totp` draft fields.
- Favicon is intentionally deferred; URL monogram remains.
