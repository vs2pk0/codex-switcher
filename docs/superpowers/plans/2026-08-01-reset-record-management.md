# Reset Record Management Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move account-level reset actions onto every reset-credit row, add editable scheduled times with confirmed cancellation, and add confirmed single/all reset-log deletion.

**Architecture:** Extend the existing Rust `ResetStateStore` with three lock-protected atomic mutations and expose them through Tauri. Keep `App.vue` as the coordinator for mutation queues and modal state, reuse `ResetScheduleModal.vue` for create/edit modes, and keep `ResetPanel.vue` as the only schedule/log management surface. Per-credit-row buttons remain account-level and never send a reset-credit ID.

**Tech Stack:** Vue 3 `<script setup>` + TypeScript, Arco Design Vue, Tauri 2, Rust, Node built-in test runner.

## Global Constraints

- Scheduling executes only while the application is running.
- Each account may have at most one active scheduled reset.
- The consume API remains account-level; new operations must not write or send `resetCreditId`.
- `reset-state.json` remains backward-compatible and reset logs remain capped at 200.
- Every destructive UI action uses a confirmation prompt.
- Do not commit, push, publish, or package unless the user explicitly requests it.
- Preserve all unrelated dirty-worktree changes.

---

## File Map

- `src-tauri/src/reset.rs`: atomic update/delete/clear mutations and storage-level tests.
- `src-tauri/src/lib.rs`: Tauri command wrappers and command registration.
- `src/services/reset.ts`: typed frontend wrappers for the new Tauri commands.
- `src/services/resetScheduleEntry.ts`: strict local datetime parse/format functions shared by create/edit mode.
- `src/services/resetUiState.ts`: reusable pending-ID guard and per-row reset action state.
- `tests/resetScheduleEntry.test.ts`: datetime edit-prefill regression tests.
- `tests/resetUiState.test.ts`: row-action and duplicate-operation guard tests.
- `src/components/ResetCreditModal.vue`: row-level immediate/schedule actions and account-level notice.
- `src/components/ResetScheduleModal.vue`: create/edit mode with prefilled time.
- `src/components/ResetPanel.vue`: edit, confirmed cancellation, confirmed log deletion, confirmed clear-all.
- `src/App.vue`: state coordination and mutation handlers.
- `src/i18n.ts`: Chinese/English labels and confirmation/error copy.
- `src/styles.css`: row action, panel toolbar, and responsive layout.

---

### Task 1: Add atomic reset-state mutations

**Files:**
- Modify: `src-tauri/src/reset.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces: `ResetStateStore::update_scheduled_reset(&self, schedule_id: &str, scheduled_at: i64, now_ms: i64) -> Result<ResetState, String>`
- Produces: `ResetStateStore::delete_log(&self, log_id: &str) -> Result<ResetState, String>`
- Produces: `ResetStateStore::clear_logs(&self) -> Result<ResetState, String>`
- Produces Tauri commands `update_codex_scheduled_reset`, `delete_codex_reset_log`, and `clear_codex_reset_logs`.

- [ ] **Step 1: Write failing storage tests for modifying a future appointment**

Add tests inside `src-tauri/src/reset.rs` that prove the public store behavior:

```rust
#[test]
fn updating_future_schedule_changes_only_its_time() {
    let dir = tempfile::tempdir().unwrap();
    let store = ResetStateStore::new(dir.path().join("reset-state.json"));
    store
        .save(&ResetState {
            scheduled_resets: vec![scheduled_task("schedule-1", 3_000)],
            logs: vec![test_log(10)],
        })
        .unwrap();

    let updated = store
        .update_scheduled_reset("schedule-1", 5_000, 2_000)
        .unwrap();

    assert_eq!(updated.scheduled_resets[0].id, "schedule-1");
    assert_eq!(updated.scheduled_resets[0].created_at, 500);
    assert_eq!(updated.scheduled_resets[0].scheduled_at, 5_000);
    assert_eq!(updated.logs, vec![test_log(10)]);
}

#[test]
fn updating_due_or_running_schedule_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let store = ResetStateStore::new(dir.path().join("reset-state.json"));
    store
        .save(&ResetState {
            scheduled_resets: vec![scheduled_task("due", 1_000)],
            logs: Vec::new(),
        })
        .unwrap();
    assert!(store
        .update_scheduled_reset("due", 4_000, 2_000)
        .unwrap_err()
        .contains("无法修改"));

    let mut running = scheduled_task("running", 4_000);
    running.status = ResetStatus::Running;
    running.started_at = Some(2_000);
    store
        .save(&ResetState {
            scheduled_resets: vec![running],
            logs: Vec::new(),
        })
        .unwrap();
    assert!(store
        .update_scheduled_reset("running", 5_000, 3_000)
        .unwrap_err()
        .contains("无法修改"));
}

#[test]
fn updating_schedule_to_past_time_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let store = ResetStateStore::new(dir.path().join("reset-state.json"));
    store
        .save(&ResetState {
            scheduled_resets: vec![scheduled_task("schedule-1", 4_000)],
            logs: Vec::new(),
        })
        .unwrap();

    let error = store
        .update_scheduled_reset("schedule-1", 2_000, 3_000)
        .unwrap_err();
    assert!(error.contains("晚于当前时间"));
    assert_eq!(store.load().unwrap().scheduled_resets[0].scheduled_at, 4_000);
}
```

- [ ] **Step 2: Run the focused Rust tests and verify RED**

Run:

```bash
cargo test --manifest-path "src-tauri/Cargo.toml" reset::tests::updating_ -- --nocapture
```

Expected: compilation fails because `update_scheduled_reset` does not exist.

- [ ] **Step 3: Implement the minimal atomic appointment update**

Add to `ResetStateStore`:

```rust
pub(crate) fn update_scheduled_reset(
    &self,
    schedule_id: &str,
    scheduled_at: i64,
    now_ms: i64,
) -> Result<ResetState, String> {
    self.mutate(|state| {
        if scheduled_at <= now_ms {
            return Err("预约时间必须晚于当前时间".to_string());
        }
        let task = state
            .scheduled_resets
            .iter_mut()
            .find(|task| task.id == schedule_id)
            .ok_or_else(|| "预约任务不存在或已无法修改".to_string())?;
        if task.status != ResetStatus::Scheduled || task.scheduled_at <= now_ms {
            return Err("预约已开始执行或已无法修改".to_string());
        }
        task.scheduled_at = scheduled_at;
        Ok(())
    })
    .map(|(state, ())| state)
}
```

- [ ] **Step 4: Verify appointment-update tests GREEN**

Run:

```bash
cargo test --manifest-path "src-tauri/Cargo.toml" reset::tests::updating_future_schedule_changes_only_its_time -- --exact --nocapture
cargo test --manifest-path "src-tauri/Cargo.toml" reset::tests::updating_due_or_running_schedule_is_rejected -- --exact --nocapture
cargo test --manifest-path "src-tauri/Cargo.toml" reset::tests::updating_schedule_to_past_time_is_rejected -- --exact --nocapture
```

Expected: all three tests pass.

- [ ] **Step 5: Write failing log deletion and clearing tests**

Add tests proving unrelated schedules/logs survive and missing IDs fail:

```rust
#[test]
fn deleting_one_log_preserves_schedules_and_other_logs() {
    let dir = tempfile::tempdir().unwrap();
    let store = ResetStateStore::new(dir.path().join("reset-state.json"));
    store
        .save(&ResetState {
            scheduled_resets: vec![scheduled_task("schedule-1", 3_000)],
            logs: vec![test_log(1), test_log(2)],
        })
        .unwrap();

    let updated = store.delete_log("log-1").unwrap();

    assert_eq!(updated.scheduled_resets.len(), 1);
    assert_eq!(updated.logs, vec![test_log(2)]);
    assert!(store.delete_log("missing").unwrap_err().contains("不存在"));
}

#[test]
fn clearing_logs_preserves_active_schedules() {
    let dir = tempfile::tempdir().unwrap();
    let store = ResetStateStore::new(dir.path().join("reset-state.json"));
    store
        .save(&ResetState {
            scheduled_resets: vec![scheduled_task("schedule-1", 3_000)],
            logs: vec![test_log(1), test_log(2)],
        })
        .unwrap();

    let updated = store.clear_logs().unwrap();

    assert!(updated.logs.is_empty());
    assert_eq!(updated.scheduled_resets[0].id, "schedule-1");
}
```

- [ ] **Step 6: Run log tests and verify RED**

Run each new test directly. Expected: compilation fails because `delete_log` and `clear_logs` do not exist.

- [ ] **Step 7: Implement minimal atomic log mutations**

Use the existing `mutate` lock/read/write boundary:

```rust
pub(crate) fn delete_log(&self, log_id: &str) -> Result<ResetState, String> {
    self.mutate(|state| {
        let position = state
            .logs
            .iter()
            .position(|log| log.id == log_id)
            .ok_or_else(|| "重置日志不存在".to_string())?;
        state.logs.remove(position);
        Ok(())
    })
    .map(|(state, ())| state)
}

pub(crate) fn clear_logs(&self) -> Result<ResetState, String> {
    self.mutate(|state| {
        state.logs.clear();
        Ok(())
    })
    .map(|(state, ())| state)
}
```

- [ ] **Step 8: Verify log tests GREEN**

Run:

```bash
cargo test --manifest-path "src-tauri/Cargo.toml" reset::tests::deleting_one_log_preserves_schedules_and_other_logs -- --exact --nocapture
cargo test --manifest-path "src-tauri/Cargo.toml" reset::tests::clearing_logs_preserves_active_schedules -- --exact --nocapture
cargo test --manifest-path "src-tauri/Cargo.toml" reset::tests -- --nocapture
```

Expected: both focused tests and the existing reset module tests pass.

- [ ] **Step 9: Expose and register Tauri commands**

Add wrappers to `src-tauri/src/lib.rs`:

```rust
#[tauri::command]
fn update_codex_scheduled_reset(
    schedule_id: String,
    scheduled_at: i64,
) -> Result<reset::ResetState, String> {
    reset::ResetStateStore::default().update_scheduled_reset(
        &schedule_id,
        scheduled_at,
        chrono::Utc::now().timestamp_millis(),
    )
}

#[tauri::command]
fn delete_codex_reset_log(log_id: String) -> Result<reset::ResetState, String> {
    reset::ResetStateStore::default().delete_log(&log_id)
}

#[tauri::command]
fn clear_codex_reset_logs() -> Result<reset::ResetState, String> {
    reset::ResetStateStore::default().clear_logs()
}
```

Register all three beside the existing reset commands in `tauri::generate_handler!`.

- [ ] **Step 10: Run Rust formatting and reset tests**

Run:

```bash
cargo fmt --manifest-path "src-tauri/Cargo.toml" -- --check
cargo test --manifest-path "src-tauri/Cargo.toml" reset::tests -- --nocapture
```

Expected: formatting check and all reset tests pass.

---

### Task 2: Add frontend command contracts and deterministic edit-prefill helpers

**Files:**
- Modify: `src/services/reset.ts`
- Modify: `src/services/resetScheduleEntry.ts`
- Modify: `tests/resetScheduleEntry.test.ts`

**Interfaces:**
- Consumes Tauri commands from Task 1.
- Produces `updateCodexScheduledReset(scheduleId: string, scheduledAt: number): Promise<ResetState>`.
- Produces `deleteCodexResetLog(logId: string): Promise<ResetState>`.
- Produces `clearCodexResetLogs(): Promise<ResetState>`.
- Produces `formatLocalScheduleInput(timestamp: number): string`.

- [ ] **Step 1: Write failing datetime formatting tests**

Extend `tests/resetScheduleEntry.test.ts`:

```ts
import {
  formatLocalScheduleInput,
  parseLocalScheduleInput,
  resolveResetScheduleEntry,
} from "../src/services/resetScheduleEntry.ts";

test("把时间戳格式化为本地分钟输入并可无损解析", () => {
  const timestamp = new Date(2026, 7, 13, 0, 32, 45, 999).getTime();
  assert.equal(formatLocalScheduleInput(timestamp), "2026-08-13 00:32");
  assert.equal(
    parseLocalScheduleInput(formatLocalScheduleInput(timestamp)),
    new Date(2026, 7, 13, 0, 32, 0, 0).getTime(),
  );
});

test("拒绝无效时间戳的格式化", () => {
  assert.equal(formatLocalScheduleInput(Number.NaN), "");
});
```

- [ ] **Step 2: Run the focused Node test and verify RED**

Run:

```bash
node --test --experimental-strip-types "tests/resetScheduleEntry.test.ts"
```

Expected: import failure because `formatLocalScheduleInput` does not exist.

- [ ] **Step 3: Implement strict local datetime formatting**

Add a zero-pad helper and format using local `Date` getters. Do not use UTC getters:

```ts
function padLocalDatePart(value: number): string {
  return String(value).padStart(2, "0");
}

export function formatLocalScheduleInput(timestamp: number): string {
  if (!Number.isFinite(timestamp)) return "";
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) return "";
  return [
    `${date.getFullYear()}-${padLocalDatePart(date.getMonth() + 1)}-${padLocalDatePart(date.getDate())}`,
    `${padLocalDatePart(date.getHours())}:${padLocalDatePart(date.getMinutes())}`,
  ].join(" ");
}
```

- [ ] **Step 4: Verify the focused Node test GREEN**

Run the same test command. Expected: all cases pass.

- [ ] **Step 5: Add typed frontend command wrappers**

Add to `src/services/reset.ts`:

```ts
export function updateCodexScheduledReset(
  scheduleId: string,
  scheduledAt: number,
): Promise<ResetState> {
  return invoke("update_codex_scheduled_reset", { scheduleId, scheduledAt });
}

export function deleteCodexResetLog(logId: string): Promise<ResetState> {
  return invoke("delete_codex_reset_log", { logId });
}

export function clearCodexResetLogs(): Promise<ResetState> {
  return invoke("clear_codex_reset_logs");
}
```

- [ ] **Step 6: Run type checking**

Run `npm run typecheck`. Expected: pass; no call sites exist yet, but command names and payload casing compile.

---

### Task 3: Define and apply per-credit-row action behavior

**Files:**
- Modify: `src/services/resetUiState.ts`
- Modify: `tests/resetUiState.test.ts`
- Modify: `src/components/ResetCreditModal.vue`
- Modify: `src/styles.css`
- Modify: `src/i18n.ts`

**Interfaces:**
- Produces `resetCreditRowActionState(available: boolean, hasScheduledReset: boolean, busy: boolean)` returning `{ consumeDisabled: boolean; scheduleDisabled: boolean; scheduleAction: "create" | "view" }`.
- Keeps existing component emits: `consume`, `open-schedule`, and `view-schedules`; none carries a reset-credit ID.

- [ ] **Step 1: Write failing row-action tests**

Extend `tests/resetUiState.test.ts`:

```ts
test("可用记录提供账号级立即重置和预约入口", () => {
  assert.deepEqual(resetCreditRowActionState(true, false, false), {
    consumeDisabled: false,
    scheduleDisabled: false,
    scheduleAction: "create",
  });
});

test("已有预约时禁止立即重置并把预约入口改为查看", () => {
  assert.deepEqual(resetCreditRowActionState(true, true, false), {
    consumeDisabled: true,
    scheduleDisabled: false,
    scheduleAction: "view",
  });
});

test("不可用记录或忙碌状态禁止发起新操作", () => {
  assert.equal(resetCreditRowActionState(false, false, false).consumeDisabled, true);
  assert.equal(resetCreditRowActionState(false, false, false).scheduleDisabled, true);
  assert.equal(resetCreditRowActionState(true, false, true).consumeDisabled, true);
  assert.equal(resetCreditRowActionState(true, false, true).scheduleDisabled, true);
});
```

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```bash
node --test --experimental-strip-types "tests/resetUiState.test.ts"
```

Expected: import failure because `resetCreditRowActionState` does not exist.

- [ ] **Step 3: Implement the minimal pure action-state function**

Return `view` whenever an active appointment exists. A view action remains enabled for available records while ordinary busy state disables creation; unavailable records remain disabled in both modes:

```ts
export interface ResetCreditRowActionState {
  consumeDisabled: boolean;
  scheduleDisabled: boolean;
  scheduleAction: "create" | "view";
}

export function resetCreditRowActionState(
  available: boolean,
  hasScheduledReset: boolean,
  busy: boolean,
): ResetCreditRowActionState {
  const scheduleAction = hasScheduledReset ? "view" : "create";
  return {
    consumeDisabled: !available || hasScheduledReset || busy,
    scheduleDisabled: !available || (scheduleAction === "create" && busy),
    scheduleAction,
  };
}
```

- [ ] **Step 4: Verify the focused test GREEN**

Run the same command. Expected: all tests pass.

- [ ] **Step 5: Move reset actions into each credit row**

In `ResetCreditModal.vue`:

- Add a visible note near the list: `按账号执行，实际消耗记录由服务端决定`.
- Add `.reset-credit-choice-actions` inside every `<article>`.
- Call the pure row-action helper with `isAvailableResetCredit(credit)`, `Boolean(scheduledReset)`, and `resetStateBusy || quotaRefreshingId === account.id`.
- Emit `consume` for immediate reset without including `credit.id`.
- Emit `open-schedule` for `create`; emit `view-schedules` for `view`.
- Keep unavailable-row buttons rendered but disabled.
- Replace the bottom three-button section with one “关闭” button.

- [ ] **Step 6: Add row-action labels and responsive styles**

Add i18n entries for the account-level notice and “关闭”. Extend the credit row grid with an actions column, use a compact horizontal button group on wide screens, and stack the action area below the row content under the existing mobile breakpoint.

- [ ] **Step 7: Run frontend tests and type checking**

Run:

```bash
npm test
npm run typecheck
```

Expected: all Node tests and Vue type checking pass.

---

### Task 4: Reuse the schedule modal for editing

**Files:**
- Modify: `src/components/ResetScheduleModal.vue`
- Modify: `src/App.vue`
- Modify: `src/services/resetUiState.ts`
- Modify: `tests/resetUiState.test.ts`
- Modify: `src/i18n.ts`

**Interfaces:**
- `ResetScheduleModal` adds props `mode: "create" | "edit"` and `initialScheduledAt?: number`.
- `ResetPanel` integration in Task 5 emits a complete `ScheduledReset` to `App.vue` for editing.
- Produces App handler `handleEditScheduledReset(task: ScheduledReset): void`.
- Produces App mutation `handleUpdateScheduledReset(scheduleId: string, scheduledAt: number): Promise<boolean>`.

- [ ] **Step 1: Generalize the pending-ID guard under a failing test**

Rename `beginScheduleCancellation` / `finishScheduleCancellation` to these general helpers in the test imports first, keeping the same duplicate-ID and precise-removal expectations:

```ts
export function beginPendingItem(activeIds: readonly string[], itemId: string): string[] | null {
  if (activeIds.includes(itemId)) return null;
  return [...activeIds, itemId];
}

export function finishPendingItem(activeIds: readonly string[], itemId: string): string[] {
  return activeIds.filter((id) => id !== itemId);
}
```

Run `node --test --experimental-strip-types "tests/resetUiState.test.ts"`; expected RED because the new exports do not exist. Then rename the production functions and existing `App.vue` cancellation call sites; run the focused test and `npm run typecheck` until GREEN.

- [ ] **Step 2: Add create/edit props and prefill behavior to the modal**

On every transition to `visible === true`:

```ts
scheduledAtInput.value = props.mode === "edit"
  ? formatLocalScheduleInput(props.initialScheduledAt ?? Number.NaN)
  : "";
```

Use mode-specific title, explanatory copy, and save-button label. Continue emitting only `save(scheduledAt)`; `App.vue` determines whether this means create or update.

- [ ] **Step 3: Add App edit target and atomic update handler**

Add:

```ts
const editingResetSchedule = ref<ScheduledReset | null>(null);
const updatingResetScheduleIds = ref<string[]>([]);
```

`handleOpenResetSchedule` clears the edit target before opening create mode. `handleEditScheduledReset` accepts only a current `scheduled` task, stores it, and opens the modal. `handleSaveResetSchedule` branches by `editingResetSchedule.value`: create uses existing `handleScheduleReset`; edit calls `updateCodexScheduledReset` through `runResetStateMutation`, guards the ID with `beginPendingItem`, and closes only after success.

- [ ] **Step 4: Bind create/edit mode into `ResetScheduleModal`**

Pass mode, initial time, account label, and saving state from `App.vue`. Closing the modal clears the edit target after the visible state becomes false so a later create opens empty.

- [ ] **Step 5: Add edit labels and error messages**

Add translations for “修改预约时间”, “保存修改”, “保存预约修改失败”, and “预约时间已更新”.

- [ ] **Step 6: Run frontend tests and type checking**

Run `npm test` and `npm run typecheck`. Expected: all pass before panel wiring.

---

### Task 5: Add confirmed schedule and log management controls

**Files:**
- Modify: `src/components/ResetPanel.vue`
- Modify: `src/App.vue`
- Modify: `src/i18n.ts`
- Modify: `src/styles.css`

**Interfaces:**
- `ResetPanel` adds props `updatingScheduleIds: string[]`, `deletingLogIds: string[]`, and `clearingLogs: boolean`.
- `ResetPanel` adds emits `edit-schedule(task: ScheduledReset)`, `delete-log(logId: string)`, and `clear-logs()`.
- Consumes Task 2 frontend command wrappers and Task 4 edit handler.

- [ ] **Step 1: Add ResetPanel event contracts before template use**

Extend props/emits, then wire `App.vue` bindings:

```vue
<ResetPanel
  :updating-schedule-ids="updatingResetScheduleIds"
  :deleting-log-ids="deletingResetLogIds"
  :clearing-logs="clearingResetLogs"
  @edit-schedule="handleEditScheduledReset"
  @delete-log="handleDeleteResetLog"
  @clear-logs="handleClearResetLogs"
/>
```

Run `npm run typecheck`. Expected RED until the new App state and handlers are added.

- [ ] **Step 2: Add confirmed appointment controls**

For each `scheduled` task:

- Add an edit button emitting the full task.
- Wrap the cancel button in `<a-popconfirm>` with `确认取消该预约？`, confirm/cancel labels, and `@ok="emit('cancel-schedule', task.id)"`.
- Do not render edit/cancel actions for `running` tasks.
- Disable both actions while that task is updating or cancelling.

- [ ] **Step 3: Add confirmed log controls**

- Change the log section header right side into a count plus “清空日志” button.
- Wrap clear in a popconfirm containing the irreversible warning; disable it when no logs exist or a clear/delete mutation is active.
- Add a danger text delete button to each log row, wrapped in its own confirmation.
- Show per-row loading for IDs in `deletingLogIds`.

- [ ] **Step 4: Implement App log mutation handlers with duplicate guards**

Add:

```ts
const deletingResetLogIds = ref<string[]>([]);
const clearingResetLogs = ref(false);
```

`handleDeleteResetLog` verifies the log still exists, uses `beginPendingItem`, calls `deleteCodexResetLog` through `runResetStateMutation`, and always removes the pending ID in `finally`. `handleClearResetLogs` returns early when already clearing or no logs exist, calls `clearCodexResetLogs`, and resets its boolean in `finally`. Display success messages only after the mutation resolves.

- [ ] **Step 5: Add i18n copy and responsive action styles**

Add translations for all edit/delete/clear labels, confirmation messages, success messages, and error prefixes. Update schedule/log grid columns to include an action group, and make the group left-aligned in the mobile one-column layout.

- [ ] **Step 6: Run the complete frontend suite and production build**

Run:

```bash
npm test
npm run build
```

Expected: all tests pass, Vue type checking passes, and Vite produces a production bundle.

---

### Task 6: Integrated verification and review

**Files:**
- Review all files listed in the File Map.
- Do not create package artifacts in this task.

**Interfaces:**
- Verifies the complete user-visible workflow and persistence boundary.

- [ ] **Step 1: Run Rust formatting and full tests**

Run:

```bash
cargo fmt --manifest-path "src-tauri/Cargo.toml" -- --check
cargo test --manifest-path "src-tauri/Cargo.toml"
```

Expected: formatting check passes and all Rust tests pass with zero failures.

- [ ] **Step 2: Run full frontend verification**

Run:

```bash
npm test
npm run build
```

Expected: all Node tests, Vue type checking, and production build pass.

- [ ] **Step 3: Inspect diff hygiene**

Run:

```bash
git diff --check
git status --short
git diff -- "src-tauri/src/reset.rs" "src-tauri/src/lib.rs" "src/services/reset.ts" "src/services/resetScheduleEntry.ts" "src/services/resetUiState.ts" "src/components/ResetCreditModal.vue" "src/components/ResetScheduleModal.vue" "src/components/ResetPanel.vue" "src/App.vue" "src/i18n.ts" "src/styles.css" "tests/resetScheduleEntry.test.ts" "tests/resetUiState.test.ts"
```

Expected: no whitespace errors, no unrelated changes introduced by this work, no secrets, and no accidental `resetCreditId` writes.

- [ ] **Step 4: Manually inspect the acceptance paths in a development run when practical**

Check one account with multiple reset-credit rows: disabled states, account-level notice, create/edit modal prefill, countdown update, confirmed cancellation, confirmed single-log deletion, and confirmed clear-all. If live account data is unavailable, report this UI runtime check as unverified rather than inferring success from compilation.

- [ ] **Step 5: Report completion without committing or packaging**

Map each design acceptance criterion to changed files and fresh verification evidence. Explicitly report any runtime path that could not be exercised.
