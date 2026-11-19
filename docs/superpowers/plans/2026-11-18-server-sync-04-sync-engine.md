# PMS Server — Implementation Plan 4: синхронизация, конфликты и квота

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Изменения уезжают на сервер сами, чужие изменения приезжают без перезапуска, конфликт двух устройств решается человеком, а в настройках видно, сколько места занято.

**Architecture:** Отдельный движок `features/sync/` со своим файлом состояния в кэше (`sync.json`: базовая версия, флаг «есть неотправленное», время последней синхронизации, ошибка). Движок ничего не знает про React: ему передают профиль, каталог кэша и `VaultStorage`, он возвращает результат, а стор и интерфейс решают, что показать. Блобы синхронизируются диффом; удаление — только по правилу 30 дней и только если блоб не нужен локально.

**Tech Stack:** TypeScript, zustand, существующие `remote_*` команды, Vitest с фейковым транспортом, Playwright для визуальной проверки.

**Spec:** `docs/superpowers/specs/2026-11-18-server-sync-design.md` (§10 синхронизация и конфликты, §10 настройки, §12)
**Предыдущие планы:** `...-01-server.md`, `...-02-client.md`, `...-03-storage-mode.md`

## Global Constraints

- `sync.json` лежит в каталоге кэша профиля: `<cache>/sync.json`, поля `baseVersion`, `dirty`, `lastSyncedAt`, `lastError`.
- Пуш — всегда с `If-Match: baseVersion`; конфликт (409) не ретраится автоматически ни разу.
- Пуш выполняется после успешного сохранения сейфа (дебаунс 1.5 с), при возврате фокуса и раз в 5 минут, пока сейф разблокирован.
- Пул — при разблокировке, перед пушем и по таймеру; при открытом редакторе/черновике обновление откладывается.
- Офлайн — не ошибка: изменения копятся, `dirty` остаётся, интерфейс пишет «есть неотправленное».
- Блоб удаляется на сервере только если он не нужен локально **и** `lastUsedAt` старше 30 дней.
- Никаких секретов в `sync.json` и в сторе синхронизации.

## Review Focus

1. Конфликт не должен разрешаться сам: ни «оставить моё», ни «взять серверное» без человека.
2. Пуш после каждого сохранения не должен гонять вложения, которых сервер уже знает.
3. Офлайн-правка, затем возврат сети: одна синхронизация, один пуш, без дублей версий.
4. Разрешение конфликта «взять серверное» не должно терять локальную копию — она уходит в `backups/`.
5. Удаление блоба не должно трогать вложение, которое ещё используется на другом устройстве (правило 30 дней).
6. Полоса квоты не должна показывать отрицательное или больше 100 % при расхождении счёта.

---

### Task 1: Состояние синхронизации в кэше

**Files:** Create `src/features/sync/state.ts`, `state.test.ts`.

**Interfaces:** `SyncState { baseVersion: number; dirty: boolean; lastSyncedAt: string | null; lastError: string | null }`; `readSyncState(storage, cacheDir): Promise<SyncState>` (по умолчанию `{baseVersion: 0, dirty: false, …}`); `writeSyncState(storage, cacheDir, state)`.

- [ ] **Step 1: Тесты** — отсутствие файла даёт дефолт; запись читается обратно; битый JSON не ломает чтение.
- [ ] **Step 2: FAIL** → реализовать через `VaultStorage.readFile/saveFile` → **PASS** → commit.

### Task 2: Движок: пул, пуш, конфликт, офлайн

**Files:** Create `src/features/sync/engine.ts`, `engine.test.ts`.

**Interfaces:**
```ts
type SyncDeps = {
  accountId: string;
  cacheDir: string;
  storage: VaultStorage;
  remote: Pick<typeof vaultApi, "remoteVaultMeta" | "remotePullVault" | "remotePushVault">;
};
type SyncOutcome =
  | { kind: "idle" } | { kind: "pulled"; version: number } | { kind: "pushed"; version: number }
  | { kind: "conflict"; remote: ServerVaultMeta }
  | { kind: "offline"; message: string } | { kind: "error"; message: string };
createSyncEngine(deps): { sync(): Promise<SyncOutcome>; markDirty(): Promise<void>; resolveKeepMine(): Promise<SyncOutcome>; resolveTakeTheirs(): Promise<SyncOutcome> };
```
- [ ] **Step 1: Тесты** (фейковый `remote` и `storage`): без изменений → `idle`; сервер новее → `pulled` и базовая версия обновилась; локально грязно и версии равны → `pushed`; сервер новее и локально грязно → `conflict` без запросов записи; `remote_unreachable` → `offline`, `dirty` сохранён; «оставить моё» → пуш с `expected = remote.version`; «взять серверное» → старая копия в `backups/`, затем `pulled`.
- [ ] **Step 2: FAIL** → реализовать → **PASS** → commit.

### Task 3: Блобы — дифф и правило 30 дней

**Files:** `src/features/sync/blobs.ts`, `blobs.test.ts`; Modify `engine.ts`.

**Interfaces:** `syncBlobs({storage, cacheDir, remote, accountId})` → `{uploaded: string[]; deleted: string[]; failed: string[]}`; `shouldDelete(blob, localIds, now)`.

- [ ] **Step 1: Тесты** — новый локальный блоб уезжает; серверный, которого нет локально и которому меньше 30 дней, остаётся; старше 30 дней — удаляется; ошибка загрузки не роняет остальные.
- [ ] **Step 2: FAIL** → реализовать → **PASS**; движок вызывает `syncBlobs` только после успешного пуша → commit.

### Task 4: Подключение к сейву и расписание

**Files:** Modify `src/features/vault/store.ts`, `src/features/sync/controller.ts` (new), `controller.test.ts`, `src/app/AppShell.tsx` или `App.tsx`.

**Interfaces:** `startSync({profile, cacheDir, storage, onStatus})` → `stop()`; статус `{kind:"idle"|"syncing"|"dirty"|"offline"|"conflict"|"error", message?}`; стор получает `remoteStatus` и `syncNow()`.

- [ ] **Step 1: Тесты** — после `save()` вызывается `markDirty` и через дебаунс один пуш; повторный `save()` во время пуша не создаёт второй запрос; фокус окна инициирует синхронизацию; при конфликте статус `conflict`.
- [ ] **Step 2: FAIL** → реализовать → **PASS** → commit.

### Task 5: Диалог конфликта

**Files:** Create `src/features/sync/ConflictDialog.tsx`, `.test.tsx`; Modify `AppShell`/`App`.

**Interfaces:** `<ConflictDialog remoteVersion localVersion onKeepMine onTakeTheirs onLater />`.

- [ ] **Step 1: Тесты** — показывает обе версии, три действия вызывают колбэки, «позже» закрывает без изменений.
- [ ] **Step 2: FAIL** → реализовать в стиле существующих модалок → **PASS** → commit.

### Task 6: Раздел сервера в настройках

**Files:** Create `src/features/settings/ServerSection.tsx`, `.test.tsx`; Modify `SettingsView.tsx`.

**Interfaces:** `<ServerSection profile status usage onSyncNow onDisconnect onDeleteAccount />`; полоса квоты `role="progressbar"` с `aria-valuenow`.

- [ ] **Step 1: Тесты** — цифры «занято X из Y», полоса заполняется пропорционально, 90 %+ краснеет, 0 % не ломается, кнопки зовут колбэки, офлайн показывает «устарело».
- [ ] **Step 2: FAIL** → реализовать → **PASS** → commit.

### Task 7: Визуальная проверка и e2e

**Files:** Create `tests/e2e/sync.spec.ts`; Modify `tests/e2e/mock-tauri.ts`.

- [ ] **Step 1: e2e** — правка записи помечает «есть неотправленное», «Синхронизировать сейчас» переводит в «синхронизировано»; конфликт открывает диалог, «взять серверное» его закрывает.
- [ ] **Step 2: Скриншоты**: настройки с полосой квоты, диалог конфликта, статус «офлайн». Посмотреть глазами, поправить.
- [ ] **Step 3: `npx vitest run`, `npx tsc --noEmit`, `npx playwright test`, `cargo test --workspace`** → commit + merge.

## Self-Review

Android-грант и биометрия — план 5. Серверные бэкапы, шаринг, поштучное слияние — вне v1 (§3). Push-уведомления не делаем: опрос по таймеру и фокусу (§3).
