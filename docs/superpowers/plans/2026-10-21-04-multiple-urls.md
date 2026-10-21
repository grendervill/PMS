# Multiple URLs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Добавить в запись PMS произвольное количество URL с обратной совместимостью со старым полем `url`.

**Architecture:** `Entry.url` остаётся legacy-зеркалом первой ссылки, `Entry.urls` становится источником истины. `EntryDraft` содержит только `urls`; редактор показывает строки, детали — все ссылки, поиск и карточка используют весь список.

**Tech Stack:** Rust/Serde, React 19, TypeScript, Vitest, Playwright, Tauri 2.

**Spec:** `docs/superpowers/specs/2026-10-21-biometrics-qr-import-design.md` (§4.1)

## Global Constraints

- Первая ссылка — основная; `Entry.url` равен `urls[0]` или `""`.
- Старые записи без `urls` читаются через legacy `url`.
- Количество ссылок не ограничено; пустые строки отбрасываются при сохранении.
- Открытие сайта по-прежнему только http/https; остальные ссылки хранятся и копируются.
- Никаких изменений Android в этом плане.

## Review Focus

- Старое хранилище с `url` без `urls` открывается и показывает ссылку.
- Несколько ссылок не дублируются и не теряются при редактировании.
- Пустые строки не превращаются в пустые ссылки.
- Поиск находит запись по любой ссылке.
- Редактор не ломается при нуле ссылок и при добавлении/удалении строк.

---

### Task 1: Rust `Entry.urls`

**Files:**
- Modify: `src-tauri/src/core/models.rs`
- Test: `src-tauri/src/core/vault.rs` (roundtrip test near existing entry tests)

**Interfaces:**
- Produces: `Entry { url: String, urls: Vec<String>, ... }` with `#[serde(default)]` on `urls`.

- [x] **Step 1: Write the failing test**

Add to `core/vault.rs` tests: create a vault, push an entry with `url: "https://a.example"` and `urls: vec!["https://a.example", "https://b.example"]`, save and unlock, assert both fields are preserved. Add a second assertion that deserializing an old `Entry` JSON with only `url` yields empty `urls`.

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml urls`
Expected: compile error because `urls` does not exist.

- [x] **Step 3: Add the field**

In `core/models.rs` add to `Entry` after `url`:

```rust
#[serde(default)]
pub urls: Vec<String>,
```

- [x] **Step 4: Run tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml urls`
Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add src-tauri/src/core/models.rs src-tauri/src/core/vault.rs
git commit -m "feat: add multiple urls to entry model"
```

### Task 2: Frontend draft/model helpers

**Files:**
- Modify: `src/features/vault/types.ts`
- Modify: `src/features/entries/entries.ts`
- Test: `src/features/entries/entries.test.ts`
- Modify fixtures: `src/features/vault/store.test.ts`, `src/features/entries/EntryDetail.test.tsx`

**Interfaces:**
- Produces: `entryUrls(entry: Pick<Entry, "url" | "urls">): string[]`.
- Produces: `EntryDraft { urls: string[]; ... }` (no `url`).
- Produces: `normalizeUrlList(values: string[]): string[]` (trim, drop empty, keep order).
- `createEntry`/`updateEntry` set `url` to first normalized URL and `urls` to the normalized list.

- [x] **Step 1: Write failing tests**

In `entries.test.ts`:
- `entryToDraft` expects `urls: []` and `url: ""` for default entry.
- Creating with `urls: ["https://a.example", " https://b.example ", ""]` yields `urls: ["https://a.example", "https://b.example"]`, `url: "https://a.example"`.
- Updating with two URLs preserves both.
- `entryUrls({url: "https://legacy", urls: []})` returns `["https://legacy"]`.
- Search finds `"b.example"` when the second URL contains it.

- [x] **Step 2: Run tests to verify they fail**

Run: `pnpm vitest run src/features/entries/entries.test.ts`
Expected: FAIL because `urls`/`entryUrls` do not exist.

- [x] **Step 3: Implement**

In `types.ts`:
```ts
url: string;
urls: string[];
```
In `entries.ts`:
```ts
export function entryUrls(entry: Pick<Entry, "url" | "urls">): string[] {
  const urls = entry.urls ?? [];
  return urls.length > 0 ? urls : entry.url ? [entry.url] : [];
}
export function normalizeUrlList(values: string[]): string[] {
  return values.map((value) => value.trim()).filter(Boolean);
}
```
Update `EntryDraft`, `emptyEntryDraft`, `normalizeEntryDraft`, `entryToDraft`, `createEntry`, `updateEntry`; search uses `entryUrls(entry).join(" ")`. Update all fixtures to `urls` in drafts.

- [x] **Step 4: Run tests**

Run: `pnpm vitest run src/features/entries/entries.test.ts src/features/vault/store.test.ts`
Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add src/features/entries src/features/vault/types.ts src/features/vault/store.test.ts
git commit -m "feat: add multiple urls to entries model"
```

### Task 3: URL editor and detail

**Files:**
- Create: `src/features/entries/UrlListInput.tsx`
- Modify: `src/features/entries/EntryEditor.tsx`
- Modify: `src/features/entries/EntryDetail.tsx`
- Modify: `src/features/entries/EntryCard.tsx`
- Test: `src/features/entries/UrlListInput.test.tsx`
- Test: `src/features/entries/EntryDetail.test.tsx`

**Interfaces:**
- Consumes: `entryUrls`, `normalizeUrlList`, `EntryDraft.urls`.
- `UrlListInput({ urls, onChange }: { urls: string[]; onChange: (urls: string[]) => void })`.
- First row label "URL"; additional rows "Дополнительный URL"; `+` has aria-label "Добавить URL"; remove has aria-label "Удалить URL".

- [x] **Step 1: Write failing component tests**

Render `UrlListInput` with `["https://a"]`; click "Добавить URL"; type into second input; click "Удалить URL" on first; assert `onChange` calls contain the expected arrays. Render `EntryDetail` with two URLs and assert both are visible.

- [x] **Step 2: Run tests to verify they fail**

Run: `pnpm vitest run src/features/entries/UrlListInput.test.tsx src/features/entries/EntryDetail.test.tsx`
Expected: FAIL because component/behavior absent.

- [x] **Step 3: Implement**

- `UrlListInput` renders one `Input` per URL plus `IconButton` remove; `+` `Button` size sm. If `urls.length === 0`, render one empty row without mutating until typing.
- `EntryEditor` uses `<UrlListInput urls={draft.urls} onChange={(urls) => update("urls", urls)} />`.
- `EntryDetail` uses `entryUrls(entry)` and renders a list of link buttons; `openSite` takes the URL and applies `safeHttpUrl`.
- `EntryCard` uses `entryUrls(entry)[0]`.

- [x] **Step 4: Run tests**

Run: `pnpm vitest run src/features/entries/UrlListInput.test.tsx src/features/entries/EntryDetail.test.tsx`
Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add src/features/entries
git commit -m "feat: edit and display multiple urls"
```

### Task 4: E2E and build

**Files:**
- Modify: `tests/e2e/vault.spec.ts`

- [x] **Step 1: Add E2E scenario**

Create an entry with URL `https://a.example`, click "Добавить URL", fill second URL `https://b.example`, save, assert both links visible; click the first link only if the mock opener handles confirmation; search for `b.example` and assert the entry is found.

- [x] **Step 2: Run E2E**

Run: `pnpm test:e2e --grep "URL"`
Expected: PASS.

- [x] **Step 3: Full verification**

Run:
```bash
pnpm typecheck
pnpm test
pnpm lint
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
pnpm tauri build
```
Expected: all pass; `.app` and `.dmg` build.

- [x] **Step 4: Visual check**

Open the Vite app with the E2E mock, create an entry with three URLs, screenshot the editor and detail; verify rows align, remove buttons are reachable, and text is not clipped. Save screenshots to `/tmp` and inspect them; remove temp files afterward.

- [x] **Step 5: Commit**

```bash
git add tests/e2e/vault.spec.ts README.md
git commit -m "test: cover multiple urls end to end"
```
