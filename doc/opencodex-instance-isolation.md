# OpenCodex 实例隔离

已实现。主实例继续使用用户目录下的 `.opencodex`；多开实例使用 Switcher 实例根目录的 `.opencodex`，不根据自定义 Codex Home 的父目录推断归属。

## 运行与存储

- 每个实例一个不可变 Backend，分别管理配置目录、Codex Home、端口、操作锁、Engine 安装目录、版本标记、日志和交互命令。
- 多开实例目录：`<switcher>/instances/<id>/.opencodex`；Engine 与管理记录：`<switcher>/instances/<id>/opencodex-manager`。
- 首次选择实例时分配并保存独立端口，避开其他实例记录和当前监听端口。
- 子进程显式接收 OPENCODEX_HOME、CODEX_HOME、CODEX_SQLITE_HOME。多开实例的 HOME、USERPROFILE、XDG 缓存目录也限定在实例根目录；不修改桌面进程的全局环境。
- 健康状态必须匹配该目录 runtime-port.json 中的端口和 PID；启动还检查实际子进程 PID。检测到端口属于其他实例/进程时拒绝操作。
- 每个实例独立安装 Engine。安装时对该实例自己的 Engine 源码副本应用服务名称适配，覆盖后台服务及探测模块的固定服务名；不修改主实例 Engine。遇到无法识别的服务结构会拒绝激活。
- macOS/Linux 服务定义仍放在实际用户的系统服务目录，名称含实例 ID，定义持久化实例环境。服务卸载只恢复并移除所选实例。删除 Codex 实例前停止其 OpenCodex 并移除其服务注册。
- 同步/恢复直接使用该实例的数据目录，不再从共享配置生成覆盖副本。

## 页面

页头实例选择控制全部子页，包括账号、模型、Engine、日志、设置和 Web Dashboard。设置按实例保存；Dashboard 窗口使用独立标签；命令事件按实例前缀过滤；异步查询回包校验实例身份。操作期间锁定选择以防误操作。

使用 imagegen 内置工具生成了紧凑布局预览，实际实现复用现有 Vue/Arco 组件，并将数据传输拆为独立组件。

预览提示词：高保真 macOS OpenCodex 实例隔离管理界面，顶部实例下拉、独立运行状态与端口，紧凑操作按钮和标签页；数据传输表单包含源/目标、复制合并/覆盖、预览表格与备份说明；Token 热力图显示精确数字悬浮提示。白底、细边框、清晰中文、紧凑布局。

## 数据传输

- 默认传输 config.json（渠道、模型等配置）、auth.json、codex-accounts.json。
- **复制合并**：同 ID 渠道/账号保留目标，账号元数据与凭证一起保留；新增项复制。其他已有目标设置优先。
- **覆盖配置和账号**：替换以上配置数据，可额外覆盖 usage.jsonl 用量历史。
- 不复制源实例端口、监听地址、管理令牌、进程记录、缓存或集成恢复身份；目标的这些内容保留。已有目标内部数据库保持完整，不做跨实例历史数据库合并。
- 源数据中指向源 OpenCodex 目录的路径改写到目标目录；发现外部绝对存储路径时拒绝传输并说明原因。
- 源目标都已安装 Engine 时要求版本相同。空目标允许先复制数据，再安装 Engine。
- 先预览条目数量；执行时核验配置及凭证指纹，过期预览不能覆盖新数据。
- 按稳定顺序获取双方操作锁，暂停原本运行的服务，构建目标快照、备份并切换目录，再恢复原运行方式。
- 目标备份保存在其 opencodex-manager 下。目录切换使用事务记录，进程中断后在下一次变更前恢复；目标重启失败尝试回滚并恢复原服务，错误及备份位置明确报告。
- 仅在用户点击执行并确认时写入真实实例数据。开发验证使用临时目录和模拟账号。

## 新版独立实例

主实例与多开实例均直接使用各自的独立服务目录，不提供旧版共享服务的迁移流程或自动回退。未安装 Engine 时显示普通安装引导，不将历史接入记录显示为已完成新实例同步。现存文件不因界面更新而自动删除或覆盖。

各实例安装并初始化自己的 Engine 后使用；需要复用数据时，在「数据传输」明确选择来源、预览并确认复制或覆盖。空目标可先接收数据，已有 Engine 的双方须使用相同版本。首次复制默认关闭目标客户端集成，待明确同步后启用。相同 OAuth 账号仍代表相同上游身份，本地隔离不能消除上游刷新令牌轮换影响。

## 界面重设计（2026-09-09）

后续调整：OpenCodex / 多开页面隐藏重复的全局标题；状态和目录使用同一行弹性布局；恢复应用原有浅色背景与共享配色变量；完成态进度条不再常驻。

停止状态允许删除当前 Engine。存在其他版本时切换到剩余版本；最后一个版本要求额外的 `removeData` 确认，后端校验状态、解除当前 Codex 接入并注销后台服务，再清理 `.opencodex` 与 `opencodex-manager`（包括备份）。目录检查保护 Codex Home、其他实例与用户目录。开发测试仅删除临时测试目录。

空目标传输会复制源 Engine 的完整已安装依赖树、重绑定服务标识并激活，无需再下载；数据失败则取消新激活并清理本次 Engine 副本。预览指纹包括双方 Engine 版本。复制保留账号、渠道、模型与支持的实例内存储路径、图片 artifacts；不复制进程、后台服务、管理令牌和集成身份，用量历史仍由覆盖选项控制。目标新服务默认停止，由用户启动。真实 Engine 回归中第二实例直接从第一实例复制、重绑定后运行，不执行下载安装。

按 imagegen 内置工具生成的 ui-mockup 设计稿实现；图像仅供预览，应用使用 Vue/CSS 原生组件。灰白底色、蓝色强调、细灰边框；版本页使用三张等宽指标卡、固定操作列；传输页使用带方向箭头的双实例卡、并排单选卡、预览表和右对齐操作区。仅覆盖模式可包含用量历史，不照搬示意图中不符合业务规则的勾选状态。

取消通用 label 布局规则，避免影响组件内部的单选框和复选框。使用容器断点适配侧栏展开及窄屏。测试夹具 `tests/fixtures/opencodex-layout.html` 完全模拟 IPC，拒绝真实写入；`tests/opencodex-layout.mjs` 检查对齐、窄屏溢出、传输预览失效、历史选项及空实例状态。

## 验证

- 前端生产构建与现有前端测试。
- Rust 单元测试和 OpenCodex 管理辅助程序测试。
- 新增数据传输回归：合并凭证一致性、覆盖保留身份、备份、过期预览、目录重叠/符号链接、外部路径、中断恢复。
- 真实 Engine 2.45.0 在两个临时实例并行运行：独立端口/PID/管理令牌，后台服务定义隔离；同步 B/恢复 B 不修改 A；停止 A 不影响 B。
- 真实 Engine 测试为显式启用的 ignored test，通过 OPENCODEX_TEST_ENGINE_DIR 和 OPENCODEX_TEST_BUN 指定本地 Engine 与 Bun；全部文件和服务进程均为临时测试数据。

Token 热力图累计视图使用 Arco Tooltip，显示日期、当日 Token、累计 Token 和请求次数的完整数字，保持现有网格尺寸。

## imagegen 预览记录

使用内置 image_gen 模式。图片仅作设计预览，不是应用运行时资源。

生成路径：`/Users/dalong/.codex/generated_images/01a07a93-fa37-7072-9db6-8cff444dcae7/exec-09fc8c19-8068-437f-a9fc-975f48a22058.png`。

最终提示词：

```text
Use case: ui-mockup
Asset type: preview-only desktop app design
Primary request: 高保真 macOS Codex Switcher 的 OpenCodex 实例隔离管理界面，紧凑工具型 UI，中文清晰文字。顶部 OpenCodex 标题、实例下拉“工作实例 1”、绿色“运行中”标记和端口 15801。下方紧凑按钮“停止”“重启”“同步 Codex”“恢复 Codex”。标签页“控制台”“账号”“图片模型”“Engine”“数据迁移”“日志”，当前数据迁移选中。内容是一张紧凑表单：源实例“默认实例”，目标实例“工作实例 1”；模式“复制合并”“覆盖数据”；说明“各实例独立存储，传输前自动备份”。下方数据预览表显示配置、账号、渠道，底部主按钮“预览传输”，次按钮“取消”。页底小型 Token 活动热力图，蓝色方格，单格悬浮提示“2026-09-09”“当日 Token 1,234,567”“累计 Token 16,900,000”“请求 128”。
Style/medium: realistic shippable compact desktop UI mockup, crisp typography, white background, fine pale blue borders, blue accent, small restrained padding, narrow left icon sidebar. Landscape. No oversized cards, no illustration, no watermark.
```
