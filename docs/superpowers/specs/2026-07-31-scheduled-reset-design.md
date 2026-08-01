# 定时重置与重置日志设计

## 背景

当前账号总览支持立即消耗单个 OAuth 账号的重置次数，但缺少预约能力、倒计时和可追溯的重置记录。Windows 构建也与 macOS 共用圆角图标资源，导致平台图标形态不符合预期。

## 目标

- 支持为单个账号预约一次重置，并选择具体日期和时间。
- 在账号相关界面和独立重置页面显示预约状态与实时倒计时。
- 记录立即重置、预约重置、失败和未执行任务。
- 任务和日志在本地持久化，应用重启后恢复未过期任务。
- Windows 使用独立方形 `.ico` 资源，macOS 保留现有 `.icns` 资源。

## 非目标与约束

- 定时任务只在 Codex Switcher 进程运行期间执行。
- 不实现系统级后台任务、开机启动、系统通知或应用关闭后的补偿执行。
- 每个账号最多保留一个活动预约，避免同一账号重复消耗重置次数。
- 预约使用本机本地时区，精确到分钟。
- 应用关闭期间到期的任务，应用下次启动时标记为“未执行”，不自动补偿。

## 用户流程

### 立即重置

1. 用户在账号卡片打开现有重置弹窗。
2. 确认账号存在可用重置次数。
3. 点击“立即重置”。
4. 调用现有 `consume_codex_reset_credit` 命令。
5. 成功或失败都写入重置日志；成功后刷新账号额度。

### 预约重置

1. 用户在现有重置弹窗中确认账号存在可用重置次数。
2. 切换到“预约重置”，选择日期和时间。
3. 保存后创建该账号的活动预约。
4. 弹窗和重置页面显示目标时间与倒计时。
5. 应用运行期间到期时，任务先变为“执行中”，随后调用现有重置命令。
6. 根据结果写入日志并更新任务状态；成功后刷新账号额度。
7. 用户可以在执行前取消预约，取消操作写入日志。

## 数据模型

新增本地持久化状态，建议由 Tauri 统一读写：

```ts
interface ScheduledReset {
  id: string;
  accountId: string;
  accountLabel: string;
  resetCreditId?: string;
  scheduledAt: number;
  status: "scheduled" | "running" | "completed" | "failed" | "missed" | "cancelled";
  createdAt: number;
  startedAt?: number;
  finishedAt?: number;
  error?: string;
}

interface ResetLog {
  id: string;
  accountId: string;
  accountLabel: string;
  type: "immediate" | "scheduled";
  resetCreditId?: string;
  occurredAt: number;
  result: "success" | "failed" | "missed" | "cancelled";
  error?: string;
}
```

持久化文件包含活动预约和最近 200 条日志。写入使用临时文件加原子替换，避免应用中断留下半写状态。应用启动时读取状态：未来的 `scheduled` 任务恢复；已过期但仍为 `scheduled` 的任务转换为 `missed` 并生成日志。

## 前端结构

- `ResetCreditModal.vue`
  - 以只读方式展示重置次数明细；消费接口不支持指定具体 credit ID。
  - 增加立即/预约模式切换。
  - 展示日期时间选择、已存在预约、预约倒计时和取消入口。
- 新增 `ResetPanel.vue`
  - 展示活动预约列表和倒计时。
  - 展示按时间倒序排列的重置日志。
  - 提供取消预约和刷新记录操作。
- `ActiveView` 增加 `resets`，并在 `AppHeader.vue` 的侧边栏和无侧边栏菜单中增加“重置记录”入口。
- 新增 `resetScheduler` 逻辑或组合式模块，负责每秒倒计时、到期任务串行执行、状态刷新和错误提示。
- `services/codex.ts` 增加预约状态的 Tauri 命令封装。
- `types/codex.ts` 或独立类型文件增加预约和日志类型。

## Tauri 结构

新增一个小型重置状态存储模块，职责限定为预约与日志文件读写，不改变现有账号数据库格式。启动迁移、常规读取和各类状态变更使用独立命令；所有变更均在 Rust 文件锁内基于最新状态完成，避免前端完整快照覆盖并发日志。提供以下命令边界：

- `get_codex_reset_state`
- `initialize_codex_reset_state`
- `create_codex_scheduled_reset`
- `cancel_codex_scheduled_reset`
- `claim_codex_scheduled_reset`
- `finish_codex_scheduled_reset`
- `append_codex_reset_log`

重置执行仍复用已有 `consume_codex_reset_credit`，避免复制鉴权和接口逻辑。保存状态时校验账号 ID、时间戳、状态枚举和日志上限；未知或损坏数据返回可识别错误，前端显示读取失败并保留空状态。

## 调度与并发

- 前端只在应用进程内运行调度器，每秒更新当前时间。
- 同一时间只执行一个到期任务，避免多个重置请求并发消耗额度。
- 执行前先持久化 `running` 状态；执行结束后持久化结果和日志。
- 执行中关闭应用时不保证请求完成；下次启动时保留该任务并将其标记为 `missed`，避免重复执行。
- 任务失败不自动重试，由日志展示错误，用户可重新预约。

## Windows 图标

- 保留现有 macOS `.icns` 资源和前端侧边栏图标。
- 从现有图标素材生成独立方形 Windows 资源，使用方形画布，透明边角，不使用 macOS 圆角蒙版。
- 更新 Tauri Windows bundle 配置，确保 `.ico` 包含 16、32、48、64、128、256 像素层级。
- 通过 `file`/图像检查确认 `.ico` 可读，并在构建产物中确认 Windows 图标配置仍然有效。

## 错误处理

- 预约时间早于当前时间时阻止保存并提示用户重新选择。
- 账号删除或重置次数失效时，预约执行失败并写入错误日志，不阻塞其他任务。
- Tauri 状态读写失败时显示错误提示，不覆盖现有本地状态。
- 重置接口失败时记录后端返回的精简错误信息，避免日志包含 access token。

## 验收标准

1. 单个账号可以创建、查看和取消一个预约重置。
2. 预约时间和倒计时在重启应用后正确恢复。
3. 到期任务只在应用运行时执行一次，并显示成功或失败结果。
4. 应用关闭期间到期的任务不会自动补偿，重启后显示为未执行。
5. 立即重置、预约重置、取消、失败和未执行均可在独立重置页面查看。
6. 日志最多保留 200 条，重启应用后仍可读取。
7. Windows 使用方形 `.ico`，macOS 继续使用现有 `.icns`。
8. `npm run typecheck`、`npm run build` 和相关 Rust 测试通过。
