# Reset Scheduling Entry Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将重置次数弹窗中的内嵌预约区替换为独立预约按钮和时间弹窗，并让已有预约按钮跳转到统一的重置记录页面。

**Architecture:** `ResetCreditModal.vue` 只负责选择重置次数与发出立即/预约/查看预约事件；新的 `ResetScheduleModal.vue` 只负责收集本地日期时间。`App.vue` 作为协调层复用现有原子预约写入逻辑，并在成功后关闭弹窗或切换到 `resets` 视图；`ResetPanel.vue` 继续作为预约时间、倒计时、取消和日志的唯一详情页面。

**Tech Stack:** Vue 3 `<script setup>`、TypeScript 5.8、Arco Design Vue、Tauri 2、Node.js 24 内置测试运行器。

## Global Constraints

- 定时任务只在 Codex Switcher 进程运行期间执行。
- 每个账号最多一个活动预约；已有预约时禁止立即重置和重复创建。
- 预约使用本机本地时区，精确到分钟。
- 不修改 Rust 数据模型、持久化格式、调度语义或平台图标资源。
- 不新增第三方依赖。
- 未经主人明确授权，不执行 `git add`、`git commit`、`git push` 或创建分支。

## File Structure

- Create `src/services/resetScheduleEntry.ts`: 预约入口决策与本地日期时间解析，保持为可独立测试的纯函数。
- Create `tests/resetScheduleEntry.test.ts`: 覆盖创建/查看入口决策与严格日期解析。
- Create `src/components/ResetScheduleModal.vue`: 独立预约时间输入弹窗。
- Modify `src/components/ResetCreditModal.vue`: 删除内嵌预约 UI，改为两个明确预约事件。
- Modify `src/App.vue`: 管理新弹窗、保存状态、成功关闭和跳转逻辑。
- Modify `src/styles.css`: 删除旧内嵌预约样式，补充独立时间弹窗与已预约按钮样式。
- Modify `src/i18n.ts`: 补充新弹窗文案的英文映射。
- Modify `package.json`: 增加无需依赖的纯函数测试命令。

---

### Task 1: 预约入口决策与日期解析

**Files:**
- Create: `tests/resetScheduleEntry.test.ts`
- Create: `src/services/resetScheduleEntry.ts`
- Modify: `package.json`

**Interfaces:**
- Produces: `resolveResetScheduleEntry(hasActiveSchedule: boolean): "create" | "view"`
- Produces: `parseLocalScheduleInput(value: string): number | null`

- [ ] **Step 1: 先写失败测试**

```ts
import test from "node:test";
import assert from "node:assert/strict";
import {
  parseLocalScheduleInput,
  resolveResetScheduleEntry,
} from "../src/services/resetScheduleEntry.ts";

test("没有活动预约时打开创建弹窗", () => {
  assert.equal(resolveResetScheduleEntry(false), "create");
});

test("存在活动预约时进入预约列表", () => {
  assert.equal(resolveResetScheduleEntry(true), "view");
});

test("严格解析本地分钟时间", () => {
  assert.equal(
    parseLocalScheduleInput("2026-08-13 00:32"),
    new Date(2026, 7, 13, 0, 32, 0, 0).getTime(),
  );
});

test("拒绝浏览器可能自动进位的无效日期", () => {
  assert.equal(parseLocalScheduleInput("2026-02-30 10:00"), null);
  assert.equal(parseLocalScheduleInput("2026-08-13T00:32"), null);
});
```

- [ ] **Step 2: 运行测试并确认因模块缺失而失败**

Run: `node --test --experimental-strip-types "tests/resetScheduleEntry.test.ts"`

Expected: FAIL，错误明确指向 `src/services/resetScheduleEntry.ts` 不存在。

- [ ] **Step 3: 实现最小纯函数**

```ts
export type ResetScheduleEntryAction = "create" | "view";

export function resolveResetScheduleEntry(hasActiveSchedule: boolean): ResetScheduleEntryAction {
  return hasActiveSchedule ? "view" : "create";
}

export function parseLocalScheduleInput(value: string): number | null {
  const match = /^(\d{4})-(\d{2})-(\d{2}) (\d{2}):(\d{2})$/.exec(value);
  if (!match) return null;
  const [, yearText, monthText, dayText, hourText, minuteText] = match;
  const year = Number(yearText);
  const month = Number(monthText);
  const day = Number(dayText);
  const hour = Number(hourText);
  const minute = Number(minuteText);
  const date = new Date(year, month - 1, day, hour, minute, 0, 0);
  if (
    date.getFullYear() !== year ||
    date.getMonth() !== month - 1 ||
    date.getDate() !== day ||
    date.getHours() !== hour ||
    date.getMinutes() !== minute
  ) return null;
  return date.getTime();
}
```

- [ ] **Step 4: 增加并运行测试脚本**

在 `package.json` 的 `scripts` 增加：

```json
"test": "node --test --experimental-strip-types \"tests/**/*.test.ts\""
```

Run: `npm test`

Expected: 4 tests PASS，0 failures。

### Task 2: 独立预约时间弹窗

**Files:**
- Create: `src/components/ResetScheduleModal.vue`
- Modify: `src/i18n.ts`
- Modify: `src/styles.css`

**Interfaces:**
- Consumes: `parseLocalScheduleInput(value: string): number | null`
- Props: `visible: boolean`, `accountLabel: string`, `saving: boolean`
- Emits: `update:visible(boolean)`, `save(number)`

- [ ] **Step 1: 创建仅负责时间输入的组件**

实现 `ResetScheduleModal.vue`：打开时清空输入；使用 `a-date-picker` 的 `YYYY-MM-DD HH:mm` 本地格式；点击保存时严格解析，解析失败提示“请选择有效的预约时间”，时间不晚于当前时提示“预约时间必须晚于当前时间”，否则发出 `save(timestamp)`；`saving` 时禁止关闭和重复提交。

- [ ] **Step 2: 补充文案与样式**

在 `src/i18n.ts` 增加“设置预约重置时间”“将为以下账号预约一次重置”“请选择有效的预约时间”的英文映射。在 `src/styles.css` 增加 `.reset-schedule-modal-body`、`.reset-schedule-account` 和 `.reset-schedule-modal-actions`，日期选择器宽度为 `100%`，弹窗内容保持现有 8px 圆角和蓝色强调风格。

- [ ] **Step 3: 运行类型检查**

Run: `npm run typecheck`

Expected: PASS，新增组件没有 Vue/TypeScript 错误。

### Task 3: 精简重置次数弹窗

**Files:**
- Modify: `src/components/ResetCreditModal.vue`
- Modify: `src/styles.css`

**Interfaces:**
- Emits: `open-schedule`, `view-schedules`
- Keeps: `consume`, `update:visible`, `update:selected-index`
- Consumes: `scheduledReset: ScheduledReset | null`

- [ ] **Step 1: 删除内嵌预约状态和展示**

删除 `mode`、`scheduledAtInput`、日期格式化/倒计时 props、`submitSchedule`、模式切换、内嵌日期框、内嵌倒计时和取消预约按钮。保留 `scheduledReset` 仅用于按钮状态与立即重置禁用判断。

- [ ] **Step 2: 增加预约入口按钮**

底部操作顺序为“取消”“立即重置”“预约重置/已预约”。无预约时按钮需要有效的已选重置次数并发出 `open-schedule`；有预约时不受重置次数选择影响，显示“已预约”及日历图标并发出 `view-schedules`。立即重置在存在活动预约时保持禁用。

- [ ] **Step 3: 清理旧样式并验证组件编译**

删除 `.reset-credit-mode-switch`、`.reset-credit-schedule-box`、`.reset-credit-date-picker`、`.reset-credit-scheduled-summary`，增加 `.reset-credit-scheduled-button` 的清晰蓝色状态样式。

Run: `npm run typecheck`

Expected: PASS，模板不再引用已删除的 props、状态和事件。

### Task 4: App 协调、成功关闭与页面跳转

**Files:**
- Modify: `src/App.vue`

**Interfaces:**
- Consumes: `ResetScheduleModal` 的 `save(number)`
- Consumes: `resolveResetScheduleEntry(Boolean(scheduledResetForModal))`
- Produces: `handleOpenResetSchedule(): void`
- Produces: `handleSaveResetSchedule(scheduledAt: number): Promise<void>`
- Produces: `handleViewResetSchedules(): void`
- Changes: `handleScheduleReset(payload): Promise<boolean>`

- [ ] **Step 1: 引入组件并添加弹窗状态**

引入 `ResetScheduleModal.vue` 和 `resolveResetScheduleEntry`；增加 `resetScheduleVisible = ref(false)`。新弹窗的账号名称来自 `resetCreditAccount`，保存态复用 `resetStateSaving`。

- [ ] **Step 2: 实现入口分流与跳转**

`handleOpenResetSchedule` 再次验证账号与可用重置次数：无活动预约时打开时间弹窗；若并发状态刷新后已存在预约，则调用查看逻辑。`handleViewResetSchedules` 关闭两个弹窗并执行 `switchView("resets")`。

- [ ] **Step 3: 让保存结果可判定**

将 `handleScheduleReset` 改为 `Promise<boolean>`：所有验证失败和捕获错误返回 `false`，持久化成功返回 `true`，并移除函数内部直接关闭重置次数弹窗的副作用。`handleSaveResetSchedule` 在 `resetStateSaving` 时直接返回；捕获当前 `resetCreditId` 调用该函数，只有返回 `true` 时才关闭两个弹窗。

- [ ] **Step 4: 更新模板事件和新弹窗**

从 `ResetCreditModal` 删除 `now-ms`、`format-countdown`、`schedule`、`cancel-schedule` 绑定，增加 `open-schedule`、`view-schedules`。在其后渲染 `ResetScheduleModal`，绑定账号名称、保存状态和 `handleSaveResetSchedule`。

- [ ] **Step 5: 验证快速重复操作保护**

确认预约时间弹窗保存按钮在 `resetStateSaving` 时 loading/disabled，`handleSaveResetSchedule` 也有同步入口保护；已有预约入口永远只导航，不调用创建命令。

Run: `npm test && npm run typecheck`

Expected: 4 tests PASS，类型检查 PASS。

### Task 5: 集成验证与差异审查

**Files:**
- Verify only: all changed files

**Interfaces:**
- No new interfaces.

- [ ] **Step 1: 运行前端完整构建**

Run: `npm run build`

Expected: `vue-tsc --noEmit` 和 `vite build` 均 PASS。

- [ ] **Step 2: 运行 Rust 回归测试**

Run: `cargo test --manifest-path "src-tauri/Cargo.toml"`

Expected: 现有预约持久化、并发领取和启动错过任务测试全部 PASS。

- [ ] **Step 3: 检查格式与工作区差异**

Run: `cargo fmt --manifest-path "src-tauri/Cargo.toml" --check`

Run: `git diff --check`

Expected: 两条命令均退出码 0；差异中没有旧内嵌预约 UI、无关重构或生成产物。

- [ ] **Step 4: 验收映射**

逐项确认：重置次数弹窗无日期/倒计时；未预约按钮打开独立弹窗；已预约按钮跳转 `resets`；列表仍显示倒计时与日志；活动预约继续禁止立即重置；所有验证命令通过。
