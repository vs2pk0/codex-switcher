# 定时重置与重置日志 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为单个 OAuth 账号增加应用运行期间的预约重置、倒计时、持久化日志，并为 Windows 使用独立方形图标资源。

**Architecture:** 新增 Rust `reset` 模块负责预约状态和日志文件的校验、读取与原子状态变更，通过细粒度 Tauri 命令暴露给前端；前端新增重置状态服务和串行调度器，在 App 生命周期内每秒检查到期任务并复用现有重置命令。重置弹窗负责创建预约，独立 `ResetPanel` 负责展示预约与日志，侧边栏新增 `resets` 视图。Windows `.ico` 从现有素材确定性生成，macOS `.icns` 保持不变。

**Tech Stack:** Vue 3 `<script setup>`, TypeScript, Arco Design Vue, Tauri 2, Rust/serde/chrono, Tauri atomic file helpers, ImageMagick or Tauri icon tooling.

## Global Constraints

- 定时任务只在 Codex Switcher 进程运行期间执行。
- 应用关闭期间到期的任务，应用下次启动时标记为“未执行”，不自动补偿。
- 每个账号最多保留一个活动预约。
- 预约使用本机本地时区，精确到分钟。
- 重置执行复用现有 `consume_codex_reset_credit`，不复制鉴权逻辑。
- 日志最多保留最近 200 条，错误信息不得包含 access token。
- 不创建 git commit、分支或 push，除非主人另行明确要求。

## File Map

- Create `src-tauri/src/reset.rs`: `ScheduledReset`、`ResetLog`、`ResetState` 类型和本地 JSON 存储、校验、200 条日志裁剪。
- Modify `src-tauri/src/lib.rs`: 注册 `reset` 模块以及读取、启动迁移、创建、取消、领取、完成和追加日志命令。
- Create `src/services/reset.ts`: Tauri invoke 封装、倒计时和任务状态纯函数。
- Create `src/types/reset.ts`: `ScheduledReset`、`ResetLog`、`ResetState` 和状态联合类型。
- Modify `src/types/ui.ts`: 增加 `resets` active view。
- Modify `src/App.vue`: 加载/保存重置状态、每秒调度、立即重置日志、预约回调和 `ResetPanel` 挂载。
- Modify `src/components/ResetCreditModal.vue`: 增加立即/预约模式、日期时间选择、已有预约和预约取消事件。
- Create `src/components/ResetPanel.vue`: 活动预约和重置日志页面。
- Modify `src/components/AppHeader.vue`: 侧边栏和无侧边栏菜单增加“重置记录”。
- Modify `src/styles.css`: 只增加重置页面、倒计时和预约状态样式，不改变全局布局约定。
- Create `src-tauri/icons/icon-windows-source.png`: 方形 Windows icon source with transparent corners。
- Modify `src-tauri/icons/icon.ico` and `src-tauri/tauri.conf.json`: 生成并引用方形 Windows icon；保留 `icon.icns`。
- Modify `src-tauri/src/reset.rs` tests or create `src-tauri/src/reset/tests.rs`: 状态迁移、校验、日志裁剪测试。

---

### Task 1: 建立重置状态存储与 Rust 单元测试

**Files:**
- Create: `src-tauri/src/reset.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/reset.rs` (`#[cfg(test)]` module)

**Interfaces:**
- Produces `ScheduledReset`, `ResetLog`, `ResetState`，字段与设计文档一致。
- Produces `ResetStateStore::load() -> Result<ResetState, String>`。
- Produces `ResetStateStore::initialize(now_ms: i64) -> Result<ResetState, String>`；常规读取使用无副作用的 `ResetStateStore::load()`。
- Produces `ResetStateStore::save(&self, state: &ResetState) -> Result<ResetState, String>`，保存前校验并裁剪日志。
- Produces `normalize_on_startup(state: ResetState, now_ms: i64) -> ResetState`，将已过期 `scheduled` 转为 `missed` 并追加日志，将 `running` 转为 `missed`。

- [ ] **Step 1: Write failing Rust tests**

在 `reset.rs` 测试模块先写以下行为测试，使用 `tempfile::tempdir()` 创建隔离目录，并让存储构造函数接收显式 `PathBuf`：

```rust
#[test]
fn startup_marks_expired_scheduled_reset_as_missed() {
    let state = ResetState {
        scheduled_resets: vec![ScheduledReset {
            id: "schedule-1".into(), account_id: "account-1".into(),
            account_label: "demo@example.com".into(), reset_credit_id: None,
            scheduled_at: 1_000, status: ResetStatus::Scheduled,
            created_at: 500, started_at: None, finished_at: None, error: None,
        }],
        logs: Vec::new(),
    };
    let normalized = normalize_on_startup(state, 2_000);
    assert_eq!(normalized.scheduled_resets[0].status, ResetStatus::Missed);
    assert_eq!(normalized.logs[0].result, ResetLogResult::Missed);
}

#[test]
fn saving_state_keeps_only_latest_200_logs() {
    let dir = tempfile::tempdir().unwrap();
    let store = ResetStateStore::new(dir.path().join("reset-state.json"));
    let logs = (0..205).map(|index| test_log(index)).collect();
    let saved = store.save(&ResetState { scheduled_resets: Vec::new(), logs }).unwrap();
    assert_eq!(saved.logs.len(), 200);
    assert_eq!(saved.logs[0].id, "log-5");
    assert!(store.path().exists());
}
```

- [ ] **Step 2: Run tests and verify the intended failure**

Run: `cargo test --manifest-path "src-tauri/Cargo.toml" reset::tests -- --nocapture`

Expected: FAIL because `ResetState`, `ResetStateStore`, `normalize_on_startup`, and the enum types do not exist yet.

- [ ] **Step 3: Implement the minimal storage module**

在 `reset.rs` 中：

1. 使用 `#[serde(rename_all = "camelCase")]` 序列化公开字段，状态枚举使用字符串枚举。
2. `ResetState::default()` 返回空预约和空日志。
3. `ResetStateStore::load` 在文件不存在时返回默认状态；读取和解析错误返回包含路径的中文错误。
4. `save` 校验 `id`、`account_id` 非空，`scheduled_at` 和时间字段非负，状态和日志结果只能来自枚举；按 `occurred_at` 升序裁剪到 200 条后用现有 `write_bytes_atomic` 语义写入临时文件并替换目标文件。
5. 将 `reset_state_path()` 放在与 `switcher_config_data_dir()` 同一应用数据目录下，必要时在 `lib.rs` 将该路径 helper 改为 `pub(crate)`，不新建第二套根目录。
6. `normalize_on_startup` 只转换 `scheduled_at <= now_ms` 的 `scheduled` 和所有 `running`，每个转换生成一条 `missed` 日志，错误文本固定为“应用未运行，任务未执行”。

- [ ] **Step 4: Run the focused tests**

Run: `cargo test --manifest-path "src-tauri/Cargo.toml" reset::tests -- --nocapture`

Expected: PASS for startup normalization, log trimming, missing-file defaults, malformed JSON errors, and invalid field rejection.

- [ ] **Step 5: Inspect the diff**

Run: `git diff --check -- "src-tauri/src/reset.rs" "src-tauri/src/lib.rs"` and确认只出现重置状态模块与必要的路径可见性调整。

### Task 2: 暴露 Tauri 命令并建立 TypeScript 服务契约

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Create: `src/services/reset.ts`
- Modify: `src/types/ui.ts`
- Test: `src-tauri/src/reset.rs` command-adjacent tests where validation is shared

**Interfaces:**
- Tauri command `get_codex_reset_state() -> Result<ResetState, String>`。
- Tauri command `initialize_codex_reset_state() -> Result<ResetState, String>`。
- Tauri commands `create_codex_scheduled_reset`、`cancel_codex_scheduled_reset`、`claim_codex_scheduled_reset`、`finish_codex_scheduled_reset`、`append_codex_reset_log`。
- TypeScript `getCodexResetState(): Promise<ResetState>`。
- TypeScript wrappers matching the atomic Tauri commands above。
- TypeScript `formatResetCountdown(remainingMs: number): string`。

- [ ] **Step 1: Add failing command/service contract checks**

先在 Rust 测试中覆盖空账号 ID和负时间戳被拒绝；在 TypeScript 服务中先声明函数签名并让 `npm run typecheck` 暴露未实现引用。

- [ ] **Step 2: Run the focused checks and verify failure**

Run: `cargo test --manifest-path "src-tauri/Cargo.toml" reset::tests -- --nocapture` and `npm run typecheck`。

Expected: Rust validation tests fail before validation实现；TypeScript 如果仅声明后不引用则不引入无意义的通过测试，需在下一步接入真实 invoke。

- [ ] **Step 3: Implement commands and service wrappers**

在 `lib.rs` 注册 `mod reset;`。常规读取无副作用，启动迁移单独调用；所有变更命令在进程级 `Mutex` 内完成读、改、写：

```rust
#[tauri::command]
fn get_codex_reset_state() -> Result<reset::ResetState, String> {
    reset::ResetStateStore::default().load()
}

#[tauri::command]
fn initialize_codex_reset_state() -> Result<reset::ResetState, String> {
    reset::ResetStateStore::default().initialize(chrono::Utc::now().timestamp_millis())
}
```

在 `src/services/reset.ts` 使用现有 `invoke` 模式封装所有原子命令；`formatResetCountdown` 返回 `Xd HH:MM:SS` 或 `HH:MM:SS`，负数按 0 处理。启动归一化只保留在 Rust 初始化入口中。

- [ ] **Step 4: Run checks**

Run: `cargo test --manifest-path "src-tauri/Cargo.toml" reset::tests -- --nocapture` and `npm run typecheck`。

Expected: PASS，且 `lib.rs` 的 `invoke_handler` 包含读取、初始化和五个原子变更命令。

### Task 3: 接入应用级调度器和立即重置日志

**Files:**
- Modify: `src/App.vue`
- Modify: `src/components/ResetCreditModal.vue`
- Modify: `src/services/reset.ts`
- Modify: `src/types/reset.ts`

**Interfaces:**
- App state `resetState`, `resetNowMs`, `resetStateLoading`, `resetSaving`。
- Modal events `schedule`, `cancel-schedule`, `consume`。
- App methods `handleScheduleReset`, `handleCancelScheduledReset`, `handleConsumeSelectedResetCredit`。
- Scheduler invariant: `running`/`scheduled` tasks are executed in a single FIFO chain and never started twice in one process.

- [ ] **Step 1: Add failing behavior fixtures**

在 `src-tauri/src/reset.rs` 增加对应的倒计时边界测试，并在 `src/services/reset.ts` 实现同一格式规则；前端通过 `npm run typecheck` 验证函数调用契约：

```rust
assert_eq!(format_reset_countdown_seconds(3_661), "01:01:01");
assert_eq!(format_reset_countdown_seconds(0), "00:00:00");
```

同时在 `ResetCreditModal.vue` 先添加事件契约，确认现有 `consume` 路径仍能编译。

- [ ] **Step 2: Run the focused check**

Run: `npm run typecheck`。

Expected: FAIL until modal props/events and App handlers are wired consistently。

- [ ] **Step 3: Implement the minimal scheduler integration**

1. `onMounted` 只调用一次 `initializeCodexResetState`；后续刷新使用无副作用的 `getCodexResetState`。
2. 使用 `window.setInterval` 每秒更新 `resetNowMs`，并在 `activeView` 无关的情况下继续调度。
3. 找出 `scheduledAt <= now` 且状态为 `scheduled` 的任务，按 `scheduledAt` 排序；通过原子领取命令写入 `running` 后再调用 `consumeCodexResetCredit(accountId)`。
4. 成功或失败时通过原子完成命令移除活动任务并追加对应日志；所有保存失败都显示错误但不吞掉重置结果。
5. 立即重置路径追加 `immediate` 日志，成功后复用现有 `loadAccounts`，失败仍保留日志。
6. 预约创建前校验无同账号 `scheduled`/`running` 任务、时间在未来、重置次数仍可用；保存后关闭弹窗并提示成功。
7. 卸载时清理 interval、调度 promise 引用和所有待处理 UI timer。

- [ ] **Step 4: Run typecheck and inspect behavior wiring**

Run: `npm run typecheck`。

Expected: PASS；检查立即重置、预约、取消和调度错误路径都通过同一个持久化服务，不在组件内直接写文件。

### Task 4: 构建重置记录页面与导航入口

**Files:**
- Create: `src/components/ResetPanel.vue`
- Modify: `src/App.vue`
- Modify: `src/components/AppHeader.vue`
- Modify: `src/styles.css`

**Interfaces:**
- `ResetPanel` props: `state`, `accounts`, `nowMs`, `loading`, `saving`。
- `ResetPanel` emits: `refresh`, `cancel-schedule`。
- `ActiveView` value `resets` is accepted by AppHeader and App switch logic.

- [ ] **Step 1: Add the failing view contract**

先在 `ActiveView` 加入 `resets`，在 App 模板中引用待创建的 `ResetPanel`，运行 typecheck 确认组件缺失导致失败。

- [ ] **Step 2: Run the focused check**

Run: `npm run typecheck`。

Expected: FAIL with missing `ResetPanel.vue` or missing required props/events。

- [ ] **Step 3: Implement `ResetPanel.vue`**

1. 顶部显示活动预约数量和“刷新”按钮。
2. 预约列表按 `scheduledAt` 升序显示账号、目标时间、`formatResetCountdown` 倒计时、状态；`scheduled` 状态显示“取消预约”图标按钮并带 tooltip，`running` 禁用取消。
3. 日志列表按 `occurredAt` 降序显示账号、时间、立即/预约、结果、错误文本；空状态使用 `a-empty`。
4. 所有动态文本通过 `t()`，按钮使用 Arco 图标，不添加嵌套卡片。

- [ ] **Step 4: Wire navigation and App lifecycle**

在 `AppHeader.vue` 的侧边栏和 `!sidebarEnabled` 菜单加入 `icon-thunderbolt` 与“重置记录”；在 App 中按 `activeView === "resets"` 挂载 `ResetPanel`，`switchView` 不触发账号或会话的无关加载。

- [ ] **Step 5: Add scoped styles and run checks**

为 `.reset-panel`、`.reset-schedule-row`、`.reset-log-row`、`.reset-countdown` 增加响应式布局，保证 1180px 最小窗口不溢出；运行 `npm run typecheck`。

Expected: PASS，导航、预约倒计时和日志在独立页面可见。

### Task 5: 扩展重置弹窗交互

**Files:**
- Modify: `src/components/ResetCreditModal.vue`
- Modify: `src/App.vue`
- Modify: `src/styles.css`

**Interfaces:**
- Modal receives `scheduledReset?: ScheduledReset | null` and `nowMs`。
- Modal emits `schedule` with `{ scheduledAt: number; resetCreditId?: string }` and `cancel-schedule`。

- [ ] **Step 1: Add failing template/type contract**

在 Modal 模板加入 `schedule` / `cancel-schedule` 事件与日期时间字段，先运行 typecheck，确认 App 尚未提供 props/handlers 时失败。

- [ ] **Step 2: Implement minimal interaction**

1. 在重置次数列表下增加 `a-radio-group`，选项为“立即重置”和“预约重置”。
2. 预约模式显示 `a-date-picker` 的 `show-time`，最小值为当前分钟；保存按钮文案为“保存预约”。
3. 有活动预约时显示目标时间和倒计时；仅 `scheduled` 状态显示取消按钮。
4. 立即模式保留现有选择和 loading/disabled 逻辑，不改变接口调用。
5. App 打开弹窗时为该账号传入当前预约，预约保存后刷新状态；取消预约写入 `cancelled` 日志并关闭弹窗。

- [ ] **Step 3: Run checks**

Run: `npm run typecheck`。

Expected: PASS；现有立即重置按钮行为保持不变，预约模式可创建和取消。

### Task 6: 生成方形 Windows 图标资源

**Files:**
- Create: `src-tauri/icons/icon-windows-source.png`
- Modify: `src-tauri/icons/icon.ico`
- Modify: `src-tauri/tauri.conf.json`

**Interfaces:**
- Windows bundle 使用方形 `icon.ico`；macOS bundle 继续使用 `icon.icns`。

- [ ] **Step 1: Prepare a deterministic square source**

从 `src-tauri/icons/icon.png` 或 `src/assets/app-icon.png` 取主体，生成 1024x1024 方形画布：保留主体颜色和比例，去除 macOS 圆角蒙版造成的椭圆/圆角外框，四角透明，主体至少留 8% 安全边距。不要重新设计图案，不调用图像生成模型。

- [ ] **Step 2: Generate and inspect the ICO**

使用仓库已有 Tauri 图标工具或 `magick` 生成包含 16、32、48、64、128、256 层级的 `src-tauri/icons/icon.ico`；运行 `file "src-tauri/icons/icon.ico"`，并用图像查看工具确认四角透明、画布为正方形。

- [ ] **Step 3: Update bundle configuration and verify**

保持 `src-tauri/tauri.conf.json` 中 macOS `icon.icns` 条目不变，确认 Windows 仍从 `icon.ico` 读取；运行 `npm run build` 检查前端构建不受资源变更影响。

### Task 7: 集成验证与回归检查

**Files:**
- Modify only if verification exposes a scoped defect in the files above。

- [ ] **Step 1: Run Rust tests**

Run: `cargo test --manifest-path "src-tauri/Cargo.toml"`

Expected: PASS，包含新增 reset 状态测试和既有 account/push/lib 测试。

- [ ] **Step 2: Run frontend typecheck and production build**

Run: `npm run typecheck` then `npm run build`

Expected: both exit 0 with no TypeScript errors.

- [ ] **Step 3: Perform a manual runtime matrix**

在开发环境逐项验证：

1. 为账号创建未来 1 分钟预约，确认弹窗和重置页面倒计时同步递减。
2. 取消预约，确认任务消失且日志出现 `cancelled`。
3. 创建到期预约，确认只调用一次重置、成功/失败状态和日志出现。
4. 关闭应用后等待预约时间，再启动应用，确认任务显示 `missed` 且没有调用接口。
5. 创建立即重置，确认成功/失败日志和账号额度刷新。
6. 重启应用，确认预约和日志仍存在；导入 205 条日志的测试状态后确认页面最多显示 200 条。
7. 检查 Windows `.ico` 是方形资源，macOS `.icns` 文件未被替换。

- [ ] **Step 4: Review final diff and status**

Run: `git diff --check`, `git status --short`, and `git diff --stat`。

Expected: 只包含本计划列出的功能文件、资源文件和设计/计划文档；不创建提交。
