# Import Framework, PMS Archive and CSV Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Добавить нормализованный импорт, импорт браузерных/простых CSV и Kaspersky, а также экспорт/импорт `.pmsvault`.

**Architecture:** Rust возвращает `ImportBundle` с записями и вложениями; React шифрует вложения существующим `attachment_seal` и сохраняет всё одним `save`. ZIP `.pmsvault` содержит manifest и уже зашифрованные файлы; импорт проверяет checksums и запрещает перезапись.

**Tech Stack:** Rust `zip`, `csv`, `sha2`, React, Tauri.

**Spec:** `docs/superpowers/specs/2026-10-21-biometrics-qr-import-design.md` (§7, §8)

## Global Constraints

- Импорт только в открытое хранилище.
- По умолчанию пропускать точные дубликаты; отчёт обязателен.
- Вложения ≤ `maxAttachmentMb`, общий лимит 500 МБ.
- `.pmsvault` не перезаписывает `pms.vault`; path traversal и zip-bomb запрещены.
- Никаких секретов в логах; plaintext-экспорт не копируется в vault.
- Android SAF не входит в план.

## Review Focus

- CSV с запятыми/кавычками внутри комментария импортируется целиком.
- Запись без имени получает осмысленное имя или попадает в отчёт.
- Вложение больше лимита пропускается, остальные записи импортируются.
- `.pmsvault` с чужим `vaultId`/плохим checksum не распаковывается.
- Импорт не сохраняет частичные записи при ошибке записи vault.

---

### Task 1: Normalized import model and CSV/browser/Kaspersky parsing

**Files:**
- Create: `src-tauri/src/core/import.rs` (or `core/import/mod.rs`)
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/commands/import.rs`
- Modify: `src-tauri/src/commands/mod.rs`, `lib.rs`
- Test: unit tests in `core/import.rs`

**Interfaces:**
- `ImportBundle { format, items, warnings }`.
- `ImportedItem { kind, name, login, password, urls, comment, totp, attachments, sshKey? }`.
- `ImportedAttachment { name, mime, b64 }`.
- `import_parse(fileB64, password?, keyfileB64?) -> ImportBundle`.
- `import_detect(fileB64) -> String` (optional internal).

- [x] **Step 1: Add `zip`/`csv` dependencies and failing tests**

Test Chrome CSV (`name,url,username,password,note`), Bitwarden CSV (`login_uri`, `login_totp`), Firefox CSV, Kaspersky TXT sample from the real format, and quoted/multiline comments. Test empty names become URL host or “Без названия” and produce a warning.

- [x] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml import`
Expected: FAIL.

- [x] **Step 3: Implement model, detection and parsers**

- CSV via `csv::ReaderBuilder` with flexible headers.
- Header aliases: name/title/account, url/uri/login_uri, username/login, password, notes/note/comment, totp/login_totp, folder, type.
- Kaspersky sections Websites/Applications/Notes, `---` separators, multiline Comment/Text.
- Detect format by headers/content.
- Decode attachments? CSV has none; Kaspersky none.

- [x] **Step 4: Add Tauri command**

`import_parse` decodes base64 and calls parser. `import_detect` can be folded into parse.

- [x] **Step 5: Run tests and clippy**

Run: `cargo test --manifest-path src-tauri/Cargo.toml import`
Run: `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`

- [x] **Step 6: Commit**

```bash
git add src-tauri
git commit -m "feat: add import model and csv parsers"
```

### Task 2: PMS archive export/import

**Files:**
- Create: `src-tauri/src/core/pms_archive.rs`
- Create: `src-tauri/src/commands/pms_archive.rs`
- Modify: `src-tauri/src/commands/mod.rs`, `lib.rs`
- Test: `core/pms_archive.rs`

**Interfaces:**
- `pms_export(folder, destination) -> ()`.
- `pms_import(archive, destination) -> ()`.
- Manifest: `format: "pmsvault"`, `version: 1`, `vaultId`, `createdAt`, `exportedAt`, `files[{path,size,sha256}]`.

- [x] **Step 1: Write failing tests**

Create temp vault folder with `pms.vault`/attachments, export, inspect manifest/checksums, import to empty folder, assert bytes equal. Test existing destination rejection, traversal entry, checksum mismatch, zip bomb limit.

- [x] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml pms_archive`
Expected: FAIL.

- [x] **Step 3: Implement**

Use `zip` store compression. Export only `pms.vault` and `attachments/`. Import into a temporary directory then rename/commit only if all checks pass; reject non-empty destination. Limit uncompressed to 2× archive or 2 GiB.

- [x] **Step 4: Run tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml pms_archive`
Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add src-tauri
git commit -m "feat: add encrypted pmsvault export and import"
```

### Task 3: Frontend import service and settings

**Files:**
- Modify: `src/features/vault/api.ts`
- Create: `src/features/import/importService.ts`
- Create: `src/features/import/ImportWizard.tsx`
- Modify: `src/features/settings/SettingsView.tsx`
- Test: `src/features/import/importService.test.ts`, `ImportWizard.test.tsx`

**Interfaces:**
- `vaultApi.importParse(file, password?, keyfile?)`, `pmsExport(folder,dest)`, `pmsImport(archive,dest)`.
- `importBundle(bundle, options, deps)` returns `ImportResult`.
- ImportWizard: file picker, password fields, preview counts, skip-duplicates checkbox, import summary.

- [x] **Step 1: Write failing tests**

CSV bundle with two entries imports; duplicate skipped/imported; attachment sealed and added; .pmsvault export/import calls API; welcome screen import path if implemented.

- [x] **Step 2: Run tests to verify they fail**

Run: `pnpm vitest run src/features/import`
Expected: FAIL.

- [x] **Step 3: Implement API and service**

`importService` seals attachments via `vaultApi.sealAttachment` + `storage.writeBlob`, builds entries with `createEntry`, SSH keys, one `save`. Do not call `addEntry` per item.

- [x] **Step 4: Implement wizard and settings buttons**

Add «Импорт из другого менеджера», «Экспорт .pmsvault», «Импорт .pmsvault» in Settings. Preview, warnings, duplicate checkbox, summary. Export/import uses save/open dialogs.

- [x] **Step 5: Run tests**

Run: `pnpm vitest run src/features/import src/features/settings`
Expected: PASS.

- [x] **Step 6: Commit**

```bash
git add src/features
git commit -m "feat: add import wizard and pms archive ui"
```

### Task 4: E2E, build, visual

- [x] **Step 1: Mock import/export commands and add E2E**
- [x] **Step 2: Full checks + build + screenshots**
- [x] **Step 3: Commit**
