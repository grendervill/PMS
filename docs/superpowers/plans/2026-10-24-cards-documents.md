# Cards and Documents Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add encrypted bank cards and identity documents (passport, SNILS, driver's license) with realistic card visuals, flip animation, card images, attachments, search, and full macOS/Android parity.

**Architecture:** Cards and documents live as new arrays in the encrypted `VaultData` payload, next to entries and SSH keys. Rust mirrors every field explicitly (the current settings model already drops `autoLock`/`cardLayout`; that is fixed here). The UI adds two sections to the existing local-state navigation, reuses the attachment pipeline for card images and document scans, and keeps cards/documents out of QR/import/audit.

**Tech Stack:** Tauri 2, React 19, TypeScript, Tailwind-style utility classes, Rust/serde, tiny attachment blobs, Vitest, Playwright, Rust tests.

**Spec:** `docs/superpowers/specs/2026-10-24-cards-documents-design.md`

## Global Constraints

- Canonical JSON field name: `bankCards` (Rust `bank_cards` with `rename_all = "camelCase"`). Never use `cards` in `VaultData`.
- `VaultData.schemaVersion` stays `1`; new arrays default to empty. Per-item `schemaVersion` is `1`.
- Card fields are stored in the encrypted vault; card images are encrypted attachments. No network requests.
- Card list masks the number to the last 4 digits; full number only in detail.
- Expiry and CVV are shown only after flipping the detail card; no hover glow.
- Holder is uppercase Latin, format "ИМЯ ФАМИЛИЯ" (at least two words).
- Supported card image types: PNG, JPEG, WebP; max 10 MB. No SVG/GIF.
- Driver's license fields follow the Russian national license: series/number, issue date, valid until, issued by, categories, full name, birth date/place, registration address.
- Mobile navigation keeps the existing four primary items; Cards and Documents live in "Ещё".
- Build both macOS and Android after the feature; APK is debug-signed.

## Review Focus

1. Opening an old vault (no `bankCards`/`documents`, no `autoLock`/`cardLayout`) must preserve settings and show empty sections.
2. Card number/CVV must never appear on the list card or the card back before an explicit flip.
3. Uploading an invalid/oversized card image must not break the editor; fallback is the selected preset.
4. Dynamic document fields must validate per kind and attachments must survive edit/save.
5. Android back must close card/document detail/editor modals before leaving the section.

---

### Task 1: Vault schema and settings persistence

**Files:**
- Modify: `src-tauri/src/core/models.rs`
- Modify: `src-tauri/src/core/vault.rs`
- Modify: `src/features/vault/types.ts`
- Modify: `src/features/settings/SettingsView.test.tsx`
- Modify: `src/features/entries/EntryDetail.test.tsx`
- Modify: `src/features/import/importService.test.ts`
- Modify: `src/features/vault/store.test.ts`
- Modify: `src/features/vault/lockFlow.test.ts`
- Modify: `src/features/vault/drafts.test.ts`
- Modify: `tests/e2e/mock-tauri.ts`

**Interfaces:**
- Produces: `BankCard`, `VaultDocument`, `VaultData.bankCards`, `VaultData.documents`, `VaultSettings.autoLock`, `VaultSettings.cardLayout` in Rust.
- Produces: same TS types in `src/features/vault/types.ts`.

- [ ] **Step 1: Write failing Rust roundtrip tests**

Add to `src-tauri/src/core/models.rs` tests:

```rust
#[test]
fn vault_data_preserves_cards_documents_and_settings() {
    let data = sample_vault_data();
    let json = serde_json::to_value(&data).unwrap();
    assert_eq!(json["bankCards"][0]["number"], "4111111111111111");
    assert_eq!(json["documents"][0]["kind"], "passport");
    assert_eq!(json["settings"]["autoLock"], "10s");
    assert_eq!(json["settings"]["cardLayout"], "advanced");
    let back: VaultData = serde_json::from_value(json).unwrap();
    assert_eq!(back, data);
}

#[test]
fn old_vault_data_defaults_new_arrays() {
    let json = serde_json::json!({
        "schemaVersion": 1,
        "id": "00000000-0000-4000-8000-000000000001",
        "name": "Old",
        "createdAt": "2026-01-01T00:00:00.000Z",
        "updatedAt": "2026-01-01T00:00:00.000Z",
        "settings": { "autoLockMinutes": 5, "clipboardClearSeconds": 30,
          "backupRetentionDays": 7, "autoBackupDays": 1, "faviconFetch": true,
          "maxAttachmentMb": 50 },
        "entries": [], "sshKeys": [], "attachments": [], "drafts": []
    });
    let data: VaultData = serde_json::from_value(json).unwrap();
    assert!(data.bank_cards.is_empty());
    assert!(data.documents.is_empty());
    assert_eq!(data.settings.auto_lock, None);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml vault_data_ -- --nocapture`
Expected: compile failure for missing fields.

- [ ] **Step 3: Implement Rust models**

Add `BankCard` and `VaultDocument` structs with camelCase serde, all string fields `#[serde(default)]`, UUID id, `image_attachment_id: Option<Uuid>`, `attachment_ids: Vec<Uuid>`, timestamps. Add `bank_cards`, `documents` to `VaultData`; add `auto_lock: Option<String>`, `card_layout: Option<String>` to `VaultSettings`. Update `Default` and `empty_vault_data`.

- [ ] **Step 4: Add TS types and update fixtures/mock**

Add `CardImagePreset`, `BankCard`, `BankCardDraft`, `DocumentKind`, `PassportKind`, `VaultDocument`, `DocumentDraft`; add `bankCards: BankCard[]`, `documents: VaultDocument[]` to `VaultData`. Add `bankCards: []`, `documents: []` to every fixture and `tests/e2e/mock-tauri.ts` `makeData`.

- [ ] **Step 5: Run tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml vault_data_` and `pnpm typecheck`.
Expected: pass.

- [ ] **Step 6: Commit**

`git add -A && git commit -m "feat: add cards and documents to vault schema"`

---

### Task 2: Card model and validation

**Files:**
- Create: `src/features/cards/cardModel.ts`
- Create: `src/features/cards/cardValidation.ts`
- Test: `src/features/cards/cardValidation.test.ts`

**Interfaces:**
- `CARD_IMAGE_PRESETS: readonly { id: CardImagePreset; label: string }[]`
- `CARD_PURPOSE_PRESETS: readonly string[]`
- `DEFAULT_CARD_PRESET: CardImagePreset`
- `emptyCardDraft(): BankCardDraft`
- `createBankCard(draft: BankCardDraft, now?: string): BankCard`
- `cardLabel(card: Pick<BankCard, "number" | "purpose">): string`
- `formatCardNumber(value: string): string`
- `maskCardNumber(value: string): string`
- `digitsOnly(value: string, max?: number): string`
- `formatExpiryInput(value: string): string`
- `normalizeHolder(value: string): string`
- `validateCardDraft(draft: BankCardDraft): Record<string, string>`

- [ ] **Step 1: Write failing tests**

Cover: digits-only filtering; groups of 4; mask `•••• •••• •••• 1234`; expiry auto-slash and month validation; holder uppercase and Latin-only; required number/expiry/CVV/holder; label fallback; create id/timestamps.

- [ ] **Step 2: Run tests to verify they fail**

Run: `pnpm vitest run src/features/cards/cardValidation.test.ts`
Expected: module not found.

- [ ] **Step 3: Implement the helpers**

Use pure functions; no React. `validateCardDraft` returns field keys `number`, `expiry`, `cvv`, `holder`. Accept 13–19 digits, CVV 3–4, holder `^[A-Z][A-Z '’-]+(?: [A-Z][A-Z '’-]+)+$` after normalization, expiry `^(0[1-9]|1[0-2])\/\d{2}$`.

- [ ] **Step 4: Run tests and typecheck**

Run: `pnpm vitest run src/features/cards/cardValidation.test.ts && pnpm typecheck`
Expected: pass.

- [ ] **Step 5: Commit**

`git add src/features/cards && git commit -m "feat: add card model and validation"`

---

### Task 3: Document model and validation

**Files:**
- Create: `src/features/documents/documentModel.ts`
- Create: `src/features/documents/documentValidation.ts`
- Test: `src/features/documents/documentValidation.test.ts`

**Interfaces:**
- `DOCUMENT_KIND_OPTIONS`, `PASSPORT_KIND_OPTIONS`, `DRIVER_CATEGORY_PRESETS`
- `emptyDocumentDraft(kind?: DocumentKind): DocumentDraft`
- `createDocument(draft: DocumentDraft, now?: string): VaultDocument`
- `documentTitle(doc: Pick<VaultDocument, "kind" | "passportKind">): string`
- `documentChip(doc: ...): string`
- `formatDocumentNumber(doc: Pick<VaultDocument, "kind" | "number">): string`
- `maskDocumentNumber(doc: ...): string`
- `formatSnils(value: string): string`
- `validateDocumentDraft(draft: DocumentDraft): Record<string, string>`

- [ ] **Step 1: Write failing tests**

Cover titles/chips for passport main/foreign/SNILS/driver; SNILS formatting; passport series/number lengths; driver required fields; date order; optional fields; create id/timestamps.

- [ ] **Step 2: Run tests to verify they fail**

Run: `pnpm vitest run src/features/documents/documentValidation.test.ts`

- [ ] **Step 3: Implement the helpers and validation matrix**

Passport main: series 4, number 6, issuedBy, departmentCode `XXX-XXX`, issuedAt, registrationAddress. Foreign: series 2, number 7, issuedBy, issuedAt. SNILS: 11 digits. Driver: series, number, issuedBy, issuedAt, validUntil, categories, fullName; optional birth date/place/address/department code. Reject `validUntil < issuedAt` and future `issuedAt`.

- [ ] **Step 4: Run tests**

`pnpm vitest run src/features/documents/documentValidation.test.ts && pnpm typecheck`

- [ ] **Step 5: Commit**

`git add src/features/documents && git commit -m "feat: add document model and validation"`

---

### Task 4: Attachment URL hook and attachments list refactor

**Files:**
- Create: `src/features/attachments/useAttachmentUrl.ts`
- Modify: `src/features/attachments/AttachmentsSection.tsx`
- Modify: `src/features/entries/EntryDetail.tsx`
- Test: `src/features/attachments/useAttachmentUrl.test.tsx` (or a small hook test)
- Test: existing `EntryDetail.test.tsx`

**Interfaces:**
- `useAttachmentUrl(attachmentId: string | null): string | null`
- `AttachmentsSection({ attachmentIds }: { attachmentIds: string[] })`

- [ ] **Step 1: Write a failing hook test**

Mock `getStorage().readBlob` and `vaultApi.openAttachment`; assert the hook returns an object URL for an image attachment and null without an id. Revoke on unmount.

- [ ] **Step 2: Implement the hook**

Use state `{ id, url }` to avoid synchronous setState in effects; create/revoke object URLs in the effect.

- [ ] **Step 3: Refactor `AttachmentsSection` to IDs**

Replace `entry` prop with `attachmentIds`; update `EntryDetail` call site. Keep the existing UI.

- [ ] **Step 4: Run tests**

`pnpm vitest run src/features/attachments src/features/entries/EntryDetail.test.tsx`

- [ ] **Step 5: Commit**

`git add src/features/attachments src/features/entries/EntryDetail.tsx && git commit -m "refactor: reuse attachments for new sections"`

---

### Task 5: Bank card art and flip

**Files:**
- Create: `src/features/cards/BankCardArt.tsx`
- Create: `src/features/cards/BankCardArt.css`
- Test: `src/features/cards/BankCardArt.test.tsx`

**Interfaces:**
- `BankCardArt({ card, imageUrl, revealNumber, flipped, className }: { card: BankCard; imageUrl: string | null; revealNumber: boolean; flipped: boolean; className?: string })`

- [ ] **Step 1: Write failing component tests**

Assert: front shows purpose, holder, masked or full number; back shows expiry and CVV; preset data attribute; custom image style; purpose absent from back.

- [ ] **Step 2: Implement the component and CSS**

Aspect ratio 1.586, container queries for type, six preset classes, custom image with overlay, front/back faces, 3D flip via `transform`, `backface-visibility`, `prefers-reduced-motion`. No interactive button inside; parent owns the flip.

- [ ] **Step 3: Run tests**

`pnpm vitest run src/features/cards/BankCardArt.test.tsx`

- [ ] **Step 4: Commit**

`git add src/features/cards/BankCardArt* && git commit -m "feat: add realistic bank card art"`

---

### Task 6: Card image picker

**Files:**
- Create: `src/features/cards/CardImagePicker.tsx`
- Test: `src/features/cards/CardImagePicker.test.tsx`

**Interfaces:**
- `CardImagePicker({ preset, attachmentId, imageUrl, onPreset, onAttachment, onError })`

- [ ] **Step 1: Write failing tests**

Preset click calls `onPreset`; upload rejects non-image/oversized; successful upload calls `sealAttachment`, writes blob, adds metadata, calls `onAttachment`; "Убрать" clears the attachment.

- [ ] **Step 2: Implement the picker**

Use `getStorage().pickFile`, `readFile`, `mimeFromName`; allow PNG/JPEG/WebP <=10MB; seal via `vaultApi.sealAttachment`, write blob, `addAttachmentMeta`, set id. Show swatches and a custom preview.

- [ ] **Step 3: Run tests**

`pnpm vitest run src/features/cards/CardImagePicker.test.tsx`

- [ ] **Step 4: Commit**

`git add src/features/cards/CardImagePicker* && git commit -m "feat: add card image picker"`

---

### Task 7: Card editor

**Files:**
- Create: `src/features/cards/CardEditor.tsx`
- Test: `src/features/cards/CardEditor.test.tsx`

**Interfaces:**
- `CardEditor({ card, onClose, onSaved }: { card: BankCard | null; onClose: () => void; onSaved?: () => void })`
- `BankCardDraft` from `vault/types`.

- [ ] **Step 1: Write failing tests**

Required fields block save; holder uppercases; number formats; preset selection; save calls `addBankCard`/`updateBankCard`; cancel with dirty asks for confirmation.

- [ ] **Step 2: Implement the editor**

Modal, live `BankCardArt` preview, `CardImagePicker`, required/optional fields, purpose presets + custom input, date-free format, validation errors, toast on failure.

- [ ] **Step 3: Run tests**

`pnpm vitest run src/features/cards/CardEditor.test.tsx`

- [ ] **Step 4: Commit**

`git add src/features/cards/CardEditor* && git commit -m "feat: add card editor"`

---

### Task 8: Cards view and detail

**Files:**
- Create: `src/features/cards/CardsView.tsx`
- Create: `src/features/cards/CardDetail.tsx`
- Maybe create: `src/features/cards/CardTile.tsx`
- Test: `src/features/cards/CardsView.test.tsx`

**Interfaces:**
- `CardsView()` renders search, grid, add, detail, editor.
- `CardDetail({ card, onClose, onEdit, onDelete }: ...)`
- Store actions: `addBankCard`, `updateBankCard`, `deleteBankCard`.

- [ ] **Step 1: Write failing tests**

Create a card, see it in the grid; search by last4/holder; open detail; flip shows CVV and expiry; delete confirms; empty state.

- [ ] **Step 2: Implement store actions and views**

Add actions to `VaultActions`/store; `CardTile` uses `useAttachmentUrl` and `BankCardArt`; `CardDetail` owns flip state, details block, copy number/CVV (CVV only flipped), edit/delete; `CardsView` handles search, modals, Android back, empty state.

- [ ] **Step 3: Run tests**

`pnpm vitest run src/features/cards/CardsView.test.tsx`

- [ ] **Step 4: Commit**

`git add src/features/cards src/features/vault/store.ts && git commit -m "feat: add cards section"`

---

### Task 9: Documents UI

**Files:**
- Create: `src/features/documents/DocumentsView.tsx`
- Create: `src/features/documents/DocumentEditor.tsx`
- Create: `src/features/documents/DocumentDetail.tsx`
- Create: `src/features/documents/DocumentTile.tsx` (optional)
- Test: `src/features/documents/DocumentsView.test.tsx`
- Test: `src/features/documents/DocumentEditor.test.tsx`

**Interfaces:**
- Store actions: `addDocument`, `updateDocument`, `deleteDocument`.
- `DocumentEditor({ document, onClose, onSaved })`
- `DocumentDetail({ document, onClose, onEdit, onDelete })`

- [ ] **Step 1: Write failing tests**

Type selector switches fields; passport main/foreign chip; SNILS formatting; driver required fields; save/attachment; search; delete.

- [ ] **Step 2: Implement tile/detail/editor/view**

Document tile with chip/title/masked number/attachment count; detail with fields and `AttachmentsSection`; editor with dynamic fields and `AttachmentPicker`; view with search, grid, modals, Android back, empty state.

- [ ] **Step 3: Run tests**

`pnpm vitest run src/features/documents`

- [ ] **Step 4: Commit**

`git add src/features/documents src/features/vault/store.ts && git commit -m "feat: add documents section"`

---

### Task 10: Navigation and shell

**Files:**
- Modify: `src/features/vault/navigation.ts`
- Modify: `src/app/AppShell.tsx`
- Test: `src/app/AppShell.test.tsx` if present, or add a navigation test.

**Interfaces:**
- `AppSection` adds `"cards"` and `"documents"`.
- `navItems` order: entries, cards, documents, totp, ssh, transfer, generator, audit, settings.

- [ ] **Step 1: Write failing navigation test**

Render shell with a mocked store; click «Карты»/«Документы»; assert sections; mobile «Ещё» contains both.

- [ ] **Step 2: Implement navigation, section metadata, empty states, render branches**

Add icons `CreditCard`, `IdCard`; keep mobile primary set; add imports/views; add `sectionMeta`/`sectionEmpty` entries.

- [ ] **Step 3: Run tests and typecheck**

`pnpm vitest run src/app src/features/vault && pnpm typecheck`

- [ ] **Step 4: Commit**

`git add src/app src/features/vault/navigation.ts && git commit -m "feat: add cards and documents navigation"`

---

### Task 11: E2E and visual checks

**Files:**
- Modify: `tests/e2e/mock-tauri.ts`
- Create: `tests/e2e/cards-documents.spec.ts`
- Maybe modify: `tests/e2e/vault.spec.ts`

- [ ] **Step 1: Extend the mock and write E2E tests**

Test card creation with a preset, front/back flip and CVV, edit/delete; passport main chip, SNILS, driver's license; document attachment; mobile «Ещё» navigation.

- [ ] **Step 2: Run E2E**

`pnpm test:e2e`
Expected: all tests pass.

- [ ] **Step 3: Screenshot and visually inspect**

Use Playwright to screenshot the card grid, detail front/back, documents grid/detail at 1200×800 and mobile 390×844; open the PNGs and check layout, clipping, contrast. Save to `/tmp` and remove after.

- [ ] **Step 4: Commit**

`git add tests && git commit -m "test: cover cards and documents"`

---

### Task 12: Build, docs, final verification

**Files:**
- Modify: `README.md`
- Modify: `docs/superpowers/specs/2026-10-24-cards-documents-design.md` if implementation changes a decision.
- Update test counts.

- [ ] **Step 1: Run all checks**

`pnpm typecheck && pnpm lint && pnpm test && pnpm test:e2e`
`cargo test --manifest-path src-tauri/Cargo.toml`
`cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
Android clippy.

- [ ] **Step 2: Build both**

`bash scripts/build-all.sh` with `ANDROID_HOME`, `NDK_HOME`, `JAVA_HOME`; copy DMG/APK to `~/Downloads`.

- [ ] **Step 3: Visual check the real macOS app**

Launch the built app, create a card with a preset and a custom image if feasible, open detail, flip, check CVV/expiry; create a passport and attach a file; screenshot the windows.

- [ ] **Step 4: Update README**

Add cards/documents features, test counts, and the settings-persistence fix.

- [ ] **Step 5: Commit**

`git add -A && git commit -m "feat: add bank cards and documents"`

---

## Self-Review

- Spec coverage: schema, settings fix, cards, images, flip, documents, attachments, navigation, mobile, tests, build.
- Type consistency: `bankCards` canonical; `BankCard`/`VaultDocument` shared names; `DocumentDraft` and `BankCardDraft` used in store/editor.
- Review focus tests assigned: old vault (Task 1), masking/flip (Tasks 5/8), image validation (Task 6), document validation/attachments (Tasks 3/9), Android back (Tasks 8/9/10).
- Non-goals kept out: QR/import/OCR/drafts/expiry reminders.
