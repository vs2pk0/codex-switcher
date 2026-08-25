import { nextTick, ref, watch } from "vue";
import type { WatchStopHandle } from "vue";
import type { Message, Modal } from "@arco-design/web-vue";

export type AppLanguage = "zh-CN" | "zh-TW" | "en" | "ru";

type ArcoMessage = typeof Message;
type ArcoModal = typeof Modal;
type MessageMethod = "info" | "success" | "warning" | "error" | "normal" | "loading";
type ModalMethod = "info" | "success" | "warning" | "error" | "confirm";
type AnyInvoker = (config: any) => unknown;

const storageKey = "codex-switcher:language";
const textNodeSources = new WeakMap<Text, string>();
const patchedMessages = new WeakSet<object>();
const patchedModals = new WeakSet<object>();
let observer: MutationObserver | null = null;
let stopLanguageWatch: WatchStopHandle | null = null;
let pendingTranslate = false;

export const currentLanguage = ref<AppLanguage>(readStoredLanguage());

export const languageOptions: Array<{ value: AppLanguage; label: string }> = [
  { value: "zh-CN", label: "中文" },
  { value: "zh-TW", label: "繁體中文（台灣）" },
  { value: "en", label: "English" },
  { value: "ru", label: "Русский" },
];

const en: Record<string, string> = {
  "中文": "Chinese",
  "繁體中文（台灣）": "Traditional Chinese (Taiwan)",
  "English": "English",
  "Русский": "Russian",
  "Codex Switcher": "Codex Switcher",
  "管理 OAuth 与 API Key 登录态，并写回本机 Codex 配置。":
    "Manage OAuth and API Key sessions, then write them back to the local Codex config.",
  "管理 OAuth 与 API Key 登录态，并写回本机 Codex 配置":
    "Manage OAuth and API Key sessions, then write them back to the local Codex config",
  "账号总览": "Accounts",
  "会话管理": "Sessions",
  "使用统计": "Usage",
  "API 服务": "API Service",
  "设置": "Settings",
  "关于": "About",
  "全部": "All",
  "当前：": "Current:",
  "异常": "Abnormal",
  "异常账号": "Abnormal Accounts",
  "有效账号": "Valid Accounts",
  "当前": "Current",
  "读取当前账号": "Detect Current",
  "已隐藏": "Hidden",
  "隐身模式": "Incognito mode",
  "隐身模式：不会进入 API 服务和 OpenCodex": "Incognito mode: excluded from API service and OpenCodex selections",
  "隐身账号不会出现在 API 服务和 OpenCodex 的账号选择列表中":
    "Incognito accounts are excluded from API service and OpenCodex account selectors",
  "已导入 OpenCodex": "Imported into OpenCodex",
  "隐私": "Privacy",
  "徽章样式": "Badge Style",
  "徽章图标样式": "Badge Icon Style",
  "{name} 的徽章样式": "{name} Badge Style",
  "30 套视觉方案": "30 visual styles",
  "经典": "Classic",
  "战术印章": "Tactical Stamp",
  "霓虹裂变": "Neon Fission",
  "琥珀闪电": "Amber Bolt",
  "轻核紫钻": "Lightcore Amethyst",
  "星翼火箭": "Starwing Rocket",
  "晶格盾章": "Lattice Shield",
  "赤羽火焰": "Crimson Flame",
  "黑曜轻翼": "Obsidian Wing",
  "鎏金王冠": "Gilded Crown",
  "量子矩阵": "Quantum Matrix",
  "等离子刃": "Plasma Blade",
  "虚空黑曜": "Void Obsidian",
  "离子脉冲": "Ion Pulse",
  "超新星核": "Nova Core",
  "焰龙熔铸": "Flameforged",
  "赛博棱镜": "Cyber Prism",
  "泰坦重甲": "Titan Armor",
  "极光镭射": "Aurora Laser",
  "陨星冲击": "Meteor Strike",
  "黑金裁决": "Blackgold Verdict",
  "冰川蓝焰": "Glacier Blue",
  "矩阵绿潮": "Matrix Tide",
  "日冕爆燃": "Solar Flare",
  "月蚀银弧": "Lunar Arc",
  "裂隙紫电": "Rift Violet",
  "巅峰红芯": "Apex Core",
  "欧米伽环": "Omega Ring",
  "顶点蓝晶": "Vertex Crystal",
  "零点推进": "Zero Thrust",
  "添加账号": "Add Account",
  "收起侧边栏": "Collapse Sidebar",
  "展开侧边栏": "Expand Sidebar",
  "全选": "Select All",
  "筛选邮箱 / 昵称": "Filter email / nickname",
  "筛选邮箱 / 昵称 / 标签": "Filter email / nickname / tag",
  "无数据": "No Data",
  "按创建时间": "By creation time",
  "按周配额": "By weekly quota",
  "按5小时配额": "By 5-hour quota",
  "按额度恢复倒计时": "By quota recovery countdown",
  "按标签": "By tag",
  "按周配额重置时间": "By weekly reset time",
  "按5小时配额重置时间": "By 5-hour reset time",
  "按订阅有效期": "By subscription expiry",
  "自定义顺序": "Custom order",
  "编辑排序": "Edit order",
  "倒序": "Desc",
  "正序": "Asc",
  "每页": "Per page",
  "卡片": "Cards",
  "紧凑": "Compact",
  "表格": "Table",
  "切换视图": "Switch View",
  "卡片视图": "Card View",
  "紧凑视图": "Compact View",
  "表格视图": "Table View",
  "绑定到 API 服务": "Bind to API Service",
  "加入了 API 服务": "Joined API service",
  "标签": "Tag",
  "选择或输入标签": "Select or enter tags",
  "批量导出": "Batch Export",
  "批量导入": "Batch Import",
  "当前账号": "Current Account",
  "当前页": "Current Page",
  "已选择": "Selected",
  "清空选择": "Clear Selection",
  "共": "Total",
  "条": "items",
  "条/页": "items/page",
  "数据": "Data",
  "应用目录": "App Directory",
  "账号 JSON": "Accounts JSON",
  "账号目录": "Account Directory",
  "会话目录": "Session Directory",
  "统计目录": "Statistics Directory",
  "配置目录": "Config Directory",
  "设置 JSON": "Settings JSON",
  "Codex 目录": "Codex Directory",
  "备份目录": "Backup Directory",
  "语言": "Language",
  "侧边栏": "Sidebar",
  "刷新": "Refresh",
  "监控额度": "Monitor Quota",
  "额度自动刷新": "Quota Auto Refresh",
  "当前账号刷新": "Current Account Refresh",
  "等待刷新倒计时": "Refresh Countdown",
  "显示 GPT 5.3 Codex Spark 额度": "Show GPT 5.3 Codex Spark Quotas",
  "保存": "Save",
  "外观": "Appearance",
  "每行固定账号数": "Fixed Accounts Per Row",
  "每页账号数": "Accounts Per Page",
  "配置": "Config",
  "推送": "Push",
  "推送设置": "Push Settings",
  "返回设置": "Back to Settings",
  "个渠道": "channels",
  "立即推送": "Push Now",
  "定时推送": "Scheduled Push",
  "推送周期": "Push Interval",
  "主动刷新额度": "Refresh Quota Before Push",
  "下次推送": "Next Push",
  "未启用": "Disabled",
  "账号规则": "Account Rules",
  "启用": "Enabled",
  "账号": "Account",
  "类型 / 绑定": "Type / Binding",
  "额度": "Quota",
  "Token 过期": "Token Expired",
  "推送渠道": "Push Channels",
  "绑定": "Bound to",
  "全部启用渠道": "All Enabled Channels",
  "添加渠道": "Add Channel",
  "渠道昵称": "Channel Name",
  "测试": "Test",
  "企业 ID": "Corp ID",
  "推送日志": "Push Logs",
  "最近": "Latest",
  "条记录": "records",
  "清空": "Clear",
  "触发": "Trigger",
  "账号 / 事件": "Account / Event",
  "渠道": "Channel",
  "结果": "Result",
  "内容": "Content",
  "响应": "Response",
  "成功": "Success",
  "手动": "Manual",
  "定时": "Scheduled",
  "还没有推送渠道": "No push channels yet",
  "还没有推送日志": "No push logs yet",
  "确认删除这个推送渠道？": "Delete this push channel?",
  "确认清空全部推送日志？": "Clear all push logs?",
  "加载推送设置失败": "Failed to load push settings",
  "推送设置尚未加载完成": "Push settings are not loaded yet",
  "请先开启定时推送": "Enable scheduled push first",
  "请至少启用一个账号推送规则": "Enable at least one account rule",
  "请至少启用一个推送渠道": "Enable at least one push channel",
  "已启用账号必须选择至少一个推送事件": "Each enabled account must select at least one push event",
  "账号规则选择的渠道均未启用": "The channels selected by an account rule are all disabled",
  "请填写": "Enter",
  "推送设置已保存": "Push settings saved",
  "保存推送设置失败": "Failed to save push settings",
  "保存的渠道不存在": "The saved channel was not returned",
  "保存的规则不存在": "The saved rule was not returned",
  "测试推送成功": "Test push succeeded",
  "测试推送失败": "Test push failed",
  "立即推送失败": "Push failed",
  "加载推送日志失败": "Failed to load push logs",
  "推送日志已清空": "Push logs cleared",
  "清空推送日志失败": "Failed to clear push logs",
  "通过规则将账号状态推送到一个或多个渠道": "Push account status to one or more channels through rules",
  "自动执行": "Automation",
  "执行全部": "Run All",
  "保存设置": "Save Settings",
  "推送规则": "Push Rules",
  "已启用规则": "Enabled Rules",
  "今日发送": "Sent Today",
  "规则列表": "Rule List",
  "规则": "Rules",
  "账号仅在规则编辑时选择，不在页面中铺开显示": "Accounts are selected only while editing a rule",
  "新增规则": "New Rule",
  "还没有推送规则": "No push rules yet",
  "规则名称": "Rule Name",
  "例如：重要账号额度预警": "For example: Important Account Quota Alert",
  "账号范围": "Account Scope",
  "触发条件": "Triggers",
  "下次检查": "Next Check",
  "个触发器": "triggers",
  "执行规则": "Run Rule",
  "确认删除这个推送规则？": "Delete this push rule?",
  "可同时启用多个条件，任一条件满足即可推送": "Enable multiple conditions; a match on any condition triggers delivery",
  "每隔": "Every",
  "剩余额度低于": "Remaining quota below",
  "剩余时间不超过": "Remaining time within",
  "检测到 Token 已失效或接口返回过期状态时推送": "Push when the token is invalid or the API reports it as expired",
  "读取额度发生错误或账号状态异常时推送": "Push when quota retrieval fails or the account is abnormal",
  "结果排序": "Result Order",
  "账号列表顺序": "Account List Order",
  "剩余额度升序": "Quota Ascending",
  "订阅到期升序": "Subscription Expiry Ascending",
  "Token 到期升序": "Token Expiry Ascending",
  "重复提醒间隔": "Repeat Interval",
  "推送前主动刷新": "Refresh Before Push",
  "关闭时直接读取定时任务保存的账号状态": "When off, use account status cached by scheduled refresh",
  "规则预览": "Rule Preview",
  "未选择渠道": "No channels selected",
  "选择一个或多个渠道": "Select one or more channels",
  "未选择账号": "No accounts selected",
  "选择要监控的账号": "Select Accounts to Monitor",
  "暂无可选账号": "No accounts available",
  "新增第一条规则": "Create First Rule",
  "渠道管理": "Channel Management",
  "规则可同时选择多个已启用渠道": "A rule can use multiple enabled channels",
  "搜索渠道": "Search channels",
  "没有匹配的渠道": "No matching channels",
  "未设置渠道昵称": "No channel name",
  "保存渠道": "Save Channel",
  "渠道已保存": "Channel saved",
  "规则 / 触发": "Rule / Trigger",
  "匹配账号 / 事件": "Matched Accounts / Events",
  "渠道测试": "Channel Test",
  "已隐藏账号": "Account hidden",
  "隐私模式下已隐藏推送内容": "Push content hidden in privacy mode",
  "额度低于": "Quota below",
  "订阅剩余": "Subscription remaining",
  "剩余": "remaining",
  "Token 已过期": "Token Expired",
  "账号异常": "Account Abnormal",
  "未设置触发条件": "No triggers configured",
  "账号状态提醒": "Account Status Alert",
  "副本": "Copy",
  "请选择至少一个账号": "Select at least one account",
  "规则包含已失效账号，请重新选择": "The rule contains unavailable accounts. Select the accounts again",
  "请选择至少一个推送渠道": "Select at least one push channel",
  "请至少启用一个触发条件": "Enable at least one trigger",
  "规则选择的渠道均未启用": "All channels selected by this rule are disabled",
  "请先新增并启用推送规则": "Create and enable a push rule first",
  "匹配账号": "Matched Accounts",
  "当前没有账号满足条件": "No accounts currently match the conditions",
  "定时状态": "Scheduled Status",
  "额度不足": "Low Quota",
  "订阅临期": "Subscription Expiring",
  "Token 临期": "Token Expiring",
  "自动": "Automatic",
  "天窗口": "day window",
  "小时窗口": "hour window",
  "分钟窗口": "minute window",
  "项目": "Projects",
  "会话": "Sessions",
  "磁盘占用": "Disk Usage",
  "项目名称": "Project Name",
  "占用": "Size",
  "更新时间": "Updated At",
  "展开": "Expand",
  "收起": "Collapse",
  "展开会话": "Expand Sessions",
  "条会话": "sessions",
  "项": "items",
  "总 Tokens": "Total Tokens",
  "总占用": "Total Size",
  "编辑 auth.json": "Edit auth.json",
  "编辑 config.toml": "Edit config.toml",
  "修复会话模型": "Repair Session Models",
  "一键修复切号会话": "Repair Switched-Account Sessions",
  "一键修复": "Repair All",
  "当前切号会话无需修复": "Switched-account sessions are already compatible",
  "切号会话一键修复完成": "Switched-account session repair completed",
  "一键修复切号会话失败": "Failed to repair switched-account sessions",
  "清理加密引用": "removed encrypted references",
  "无论从正常账号切到 API 服务账号，还是再切回来，都会按当前账号双向修复 Provider 模型前缀、线程模型与推理强度，并清理上一个账号遗留的 cmp/rs 加密引用。只移除无法跨账号复用的推理和压缩元数据，用户消息、助手回复与工具记录都会保留；修复前会自动备份，期间 ChatGPT/Codex 会自动重启。":
    "Whether switching from an official account to an API service account or back again, repair provider-qualified model names in both directions, reset thread model and reasoning settings for the current account, and remove cmp/rs encrypted references left by the previous account. Only reasoning and compaction metadata that cannot be reused across accounts is removed; user messages, assistant replies, and tool records are preserved. A backup is created first, and ChatGPT/Codex restarts automatically.",
  "将按当前账号清理本地会话中残留的旧 Provider 模型前缀，重置显式模型与推理强度，并同步 Provider。修复前会备份会话文件和数据库，不会删除会话内容。修复期间 ChatGPT/Codex 会自动重启。":
    "Remove stale provider prefixes from local session models, reset explicit model and reasoning selections, and synchronize the provider with the current account. Session files and databases are backed up first; session content is not deleted. ChatGPT/Codex restarts automatically during repair.",
  "开始修复": "Start Repair",
  "当前会话模型配置无需修复": "Session model settings are already compatible",
  "会话模型修复完成": "Session model repair completed",
  "修复会话模型失败": "Failed to repair session models",
  "个会话文件": "session files",
  "直接查看并编辑本机 Codex 配置文件，保存前会检查格式并自动备份原文件。":
    "View and edit local Codex configuration files. Their format is checked and the original file is backed up before saving.",
  "文件已存在": "File exists",
  "文件不存在，保存时会创建": "The file does not exist and will be created when saved",
  "auth.json 包含登录令牌等敏感信息，请勿截图、复制或分享给他人。":
    "auth.json contains sensitive credentials such as login tokens. Do not screenshot, copy, or share it.",
  "请输入配置内容": "Enter configuration content",
  "查找配置内容": "Find in configuration",
  "上一个匹配": "Previous match",
  "下一个匹配": "Next match",
  "关闭查找": "Close find",
  "API Key 模型列表": "API Key Models",
  "目标账号": "Target account",
  "当前默认": "Current default",
  "筛选模型名称": "Filter model names",
  "重新获取列表": "Refresh list",
  "获取模型列表": "Fetch models",
  "正在读取本机 Codex 配置并核对当前 API Key…": "Reading the local Codex configuration and checking the active API Key…",
  "当前 API Key 与目标账号匹配，可以设置模型。": "The active API Key matches this account. You can set its model.",
  "当前 Codex 配置不是此 API Key，请先切换到该账号后再设置模型。":
    "The active Codex configuration does not use this API Key. Switch to this account before setting its model.",
  "读取本机 Codex 配置失败": "Failed to read the local Codex configuration",
  "重新检测": "Check again",
  "提供方": "Provider",
  "5.6 配置": "5.6 config",
  "没有匹配的模型": "No matching models",
  "点击“获取模型列表”从当前 API 服务读取可用模型": "Click “Fetch models” to load available models from this API service",
  "API 服务没有返回可用模型": "The API service returned no available models",
  "获取模型列表失败": "Failed to fetch the model list",
  "请选择默认模型": "Select a default model",
  "默认模型已保存并写入 config.toml": "The default model was saved to config.toml",
  "保存默认模型失败": "Failed to save the default model",
  "选择 5.6 系列模型后，将同步写入 Responses 模式与兼容配置，并移除旧 WebSocket 配置。":
    "Selecting a 5.6 model writes the Responses compatibility settings and removes legacy WebSocket settings.",
  "设为默认模型": "Set as default",
  "格式化并检查": "Format & Validate",
  "重新加载": "Reload",
  "保存文件": "Save File",
  "重置 config.toml": "Reset config.toml",
  "删除 config.toml": "Delete config.toml",
  "确认将本机 Codex 目录下的 config.toml 恢复为内置基础配置？当前文件会先自动备份。":
    "Restore config.toml under the local Codex directory to the built-in baseline? The current file will be backed up first.",
  "恢复基础配置": "Restore Baseline",
  "已将 config.toml 恢复为基础配置": "config.toml was restored to the baseline configuration",
  "确认永久删除本机 Codex 目录下的 config.toml？此操作只删除当前配置文件，不会自动恢复基础配置。":
    "Permanently delete config.toml under the local Codex directory? This only deletes the current file and will not restore the baseline configuration.",
  "已删除 config.toml": "config.toml was deleted",
  "config.toml 不存在，无需删除": "config.toml does not exist; nothing was deleted",
  "备份": "Backup",
  "打开备份目录": "Open Backup Directory",
  "恢复": "Restore",
  "删除": "Delete",
  "开始恢复": "Start Restore",
  "恢复完整备份": "Restore Full Backup",
  "还没有备份": "No backups yet",
  "点击“手动备份”会把账号、设置、统计缓存、费用规则与所有 Codex 会话记录打包成 ZIP。":
    "Click \"Manual Backup\" to package accounts, settings, statistics cache, pricing rules, and all Codex sessions into a ZIP.",
  "确认删除这个备份文件？": "Delete this backup file?",
  "手动备份": "Manual Backup",
  "分钟": "minutes",
  "小时": "hours",
  "天": "days",
  "万": "ten-thousand",
  "亿": "hundred-million",
  "已过期": "Expired",
  "已记录": "Recorded",
  "等待刷新": "Waiting",
  "个": "items",
  "重置会删除本机 Codex 目录下的 config.toml，适合切换配置异常时恢复默认配置。":
    "Reset deletes config.toml under the local Codex directory, useful for restoring defaults after switching issues.",
  "取消": "Cancel",
  "确认": "Confirm",
  "关闭": "Close",
  "导入": "Import",
  "添加": "Add",
  "编辑": "Edit",
  "表单": "Form",
  "JSON": "JSON",
  "Base URL": "Base URL",
  "API Key": "API Key",
  "接入新账号": "Add Account",
  "重新授权账号": "Reauthorize Account",
  "编辑账号": "Edit Account",
  "编辑 API Key": "Edit API Key",
  "编辑 OAuth 账号": "Edit OAuth Account",
  "切换": "Switch",
  "置顶账号": "Pin account",
  "取消置顶": "Unpin account",
  "绑定手机": "Bind Phone",
  "未命名账号": "Unnamed Account",
  "未设置": "Not Set",
  "未保存": "Not Saved",
  "邮箱": "Email",
  "订阅": "Subscription",
  "订阅信息": "Subscription Info",
  "配额状态": "Quota Status",
  "配额 / 余额": "Quota / Balance",
  "余额": "Balance",
  "密钥额度": "Key quota",
  "套餐剩余": "Plan remaining",
  "刷新余额": "Refresh balance",
  "点击刷新余额": "Click to refresh balance",
  "余额获取中": "Loading balance",
  "余额获取失败": "Failed to load balance",
  "余额获取失败，点击重试": "Failed to load balance. Click to retry",
  "余额已刷新": "Balance refreshed",
  "需确认": "Confirm",
  "HTTP 地址需点击确认后查询余额": "HTTP address requires confirmation. Click to query balance",
  "HTTP 连接安全提示": "HTTP connection security warning",
  "该中转站使用未加密的 HTTP 连接。查询余额时会通过此连接发送 API Key，同一网络中的设备或代理可能读取它。是否仅在本次运行期间允许？":
    "This relay uses an unencrypted HTTP connection. Querying the balance sends the API Key through it, so devices or proxies on the same network may read it. Allow only for this app run?",
  "仅本次运行允许": "Allow for this run only",
  "账号配置已变化，请重新点击余额并确认": "The account configuration changed. Click the balance and confirm again",
  "获取失败": "Unavailable",
  "更新于": "Updated",
  "不限量": "Unlimited",
  "未获得订阅信息": "No subscription info",
  "OAuth 登录": "OAuth Login",
  "Token 登录": "Token Login",
  "用户 ID": "User ID",
  "账号 ID": "Account ID",
  "周配额": "Weekly",
  "可用重置次数": "Available reset credits",
  "刷新额度": "Refresh Quota",
  "刷新全部额度": "Refresh All Quotas",
  "请先在设置中开启额度监控": "Enable quota monitoring in Settings first",
  "没有可刷新的 OAuth 账号": "No OAuth accounts to refresh",
  "重置额度": "Reset Quota",
  "导出": "Export",
  "重新授权": "Reauthorize",
  "官网地址": "Official Website",
  "额度概览": "Quota Overview",
  "自动同步": "Auto Sync",
  "长周期": "Long Window",
  "短周期": "Short Window",
  "订阅状态": "Subscription",
  "密钥状态": "Key Status",
  "Token 可用": "Token Available",
  "Token 失效": "Token Expired",
  "Token 失效，请重新登录或更换绑定账号": "Token expired. Please reauthorize or switch the bound account.",
  "请重新登录或更换绑定账号": "Please reauthorize or switch the bound account",
  "绑定 OAuth": "Bind OAuth",
  "未绑定": "Unbound",
  "消耗看板": "Usage Dashboard",
  "从本机会话记录汇总 Tokens、缓存复用和预估费用":
    "Summarize tokens, cache reuse, and estimated cost from local sessions.",
  "当天": "Today",
  "昨天": "Yesterday",
  "前天": "Two days ago",
  "当月": "This Month",
  "上月": "Last Month",
  "请选择有效的开始和结束时间，日期必须真实存在且开始时间不能晚于结束时间。":
    "Select a valid start and end time. The dates must exist and the start cannot be later than the end.",
  "本地 Codex 消耗": "Local Codex Usage",
  "总请求数": "Requests",
  "预估费用": "Estimated Cost",
  "新增输入 Tokens": "New Input Tokens",
  "输出 Tokens": "Output Tokens",
  "缓存写入": "Cache Write",
  "缓存复用": "Cache Reuse",
  "输入": "Input",
  "输出": "Output",
  "平均": "avg",
  "复用占比": "Reuse Rate",
  "趋势": "Trend",
  "时段消耗曲线": "Usage by Time Range",
  "暂无趋势数据": "No trend data",
  "累计 Token 数": "Total Tokens",
  "峰值 Token 数": "Peak Tokens",
  "当前连续天数": "Current Streak",
  "最长连续天数": "Longest Streak",
  "Token 活动": "Token Activity",
  "凌晨": "Late Night",
  "清晨": "Early Morning",
  "上午": "Morning",
  "午后": "Afternoon",
  "傍晚": "Evening",
  "夜间": "Night",
  "每日": "Daily",
  "每周": "Weekly",
  "累计": "Total",
  "时段": "Period",
  "当日": "Day",
  "暂无活动数据": "No activity data",
  "调用流水": "Call Logs",
  "来源汇总": "Sources",
  "模型用量": "Models",
  "调用记录": "Call Records",
  "时间": "Time",
  "来源": "Source",
  "计费模型": "Billing Model",
  "费用": "Cost",
  "状态": "Status",
  "暂无使用记录": "No usage records",
  "第": "Page",
  "页": "Page",
  "请求数": "Requests",
  "Tokens": "Tokens",
  "成功率": "Success Rate",
  "平均延迟": "Avg Latency",
  "模型": "Model",
  "单次均价": "Avg Cost",
  "本期概览": "Period Overview",
  "主要来源": "Top Source",
  "主要模型": "Top Model",
  "费用倍率": "Cost Multiplier",
  "请求模型": "Request Model",
  "返回模型": "Response Model",
  "来源分布": "Source Distribution",
  "模型分布": "Model Distribution",
  "暂无来源数据": "No source data",
  "暂无模型用量": "No model usage",
  "请求明细": "Request Logs",
  "Provider 统计": "Provider Stats",
  "模型统计": "Model Stats",
  "费用规则": "Pricing",
  "维护 Codex 统计使用的模型单价和倍率": "Maintain model prices and multipliers for Codex usage statistics",
  "Codex 计费口径": "Codex Billing Basis",
  "设置统计倍率与模型识别来源": "Set the statistics multiplier and model source",
  "应用": "App",
  "默认倍率": "Default Multiplier",
  "计费模式": "Billing Mode",
  "模型单价（每百万 Tokens）": "Model Pricing (per 1M Tokens)",
  "条规则": "rules",
  "恢复默认": "Restore Defaults",
  "显示名称": "Display Name",
  "输入单价": "Input Price",
  "输出单价": "Output Price",
  "操作": "Actions",
  "暂无模型单价": "No model pricing",
  "确定删除这条模型单价？": "Delete this model pricing rule?",
  "编辑模型单价": "Edit Model Pricing",
  "添加模型单价": "Add Model Pricing",
  "模型 ID": "Model ID",
  "输入单价 / 1M": "Input Price / 1M",
  "输出单价 / 1M": "Output Price / 1M",
  "缓存复用 / 1M": "Cache Read / 1M",
  "缓存写入 / 1M": "Cache Write / 1M",
  "例如 gpt-5-codex": "e.g. gpt-5-codex",
  "例如 GPT-5 Codex": "e.g. GPT-5 Codex",
  "恢复内置 GPT/Codex 单价会覆盖当前 pricing.json，确定继续？":
    "Restoring built-in GPT/Codex pricing will overwrite pricing.json. Continue?",
  "服务状态": "Service Status",
  "当前版本": "Current Version",
  "访问地址": "Endpoint",
  "未安装": "Not Installed",
  "已安装": "Installed",
  "运行中": "Running",
  "开启服务": "Start Service",
  "下载并开启": "Download and Start",
  "停止服务": "Stop Service",
  "检测更新": "Check Updates",
  "下载更新": "Download Update",
  "版本管理": "Version Manager",
  "API 服务版本管理": "API Service Version Manager",
  "本地版本": "Local Versions",
  "更新或导入成功后默认切换到本机最高版本，并保留当前版本和最多 2 个旧版本。":
    "After an update or import, the highest local version becomes current. The current version and up to two older versions are retained.",
  "导入本地包": "Import Local Package",
  "GitHub 更新接口当前限流，可打开“版本管理”导入本地安装包":
    "GitHub update requests are currently rate-limited. Open Version Manager to import a local package.",
  "CLIProxyAPI 安装包": "CLIProxyAPI Package",
  "导入本地版本包": "Import Local Version Package",
  "手动导入用于 GitHub 限流或网络不可用的情况，不会通过 GitHub checksums 在线校验。请只导入可信的 CLIProxyAPI 官方安装包。导入成功后会自动使用本机最高版本，并仅保留当前版本和 2 个旧版本。":
    "Manual import is intended for GitHub rate limits or unavailable networks and does not verify GitHub checksums online. Only import trusted official CLIProxyAPI packages. After import, the highest local version becomes current and only the current version plus two older versions are retained.",
  "确认导入": "Confirm Import",
  "版本包已导入，已切换到本机最新版本":
    "The package was imported and the newest local version is now current.",
  "导入失败：{error}": "Import failed: {error}",
  "选择版本包失败：{error}": "Could not select the version package: {error}",
  "切换到 v{version}": "Switch to v{version}",
  "API 服务正在运行，切换时会自动重启；如果新版本启动失败，将恢复当前版本。是否继续？":
    "The API service is running and will restart during the switch. If the selected version fails to start, the current version will be restored. Continue?",
  "切换后，该版本会成为下次启动 API 服务时使用的版本。是否继续？":
    "This version will be used the next time the API service starts. Continue?",
  "已切换到 v{version}": "Switched to v{version}",
  "切换失败：{error}": "Switch failed: {error}",
  "删除 v{version}": "Delete v{version}",
  "将删除这个本地运行时版本，当前使用的版本不会受影响。是否继续？":
    "This local runtime version will be deleted. The current version will not be affected. Continue?",
  "已删除 v{version}": "Deleted v{version}",
  "删除失败：{error}": "Delete failed: {error}",
  "旧版本清理尚未完成：当前有 {count} 个旧版本，最多应保留 {limit} 个。请稍后重试删除。":
    "Old-version cleanup is incomplete: {count} old versions remain, but at most {limit} should be kept. Please retry deletion later.",
  "版本": "Version",
  "平台": "Platform",
  "导入时间": "Imported At",
  "包文件": "Package",
  "状态与操作": "Status & Actions",
  "设为当前": "Set as Current",
  "平台不兼容": "Incompatible Platform",
  "不可删除": "Cannot Delete",
  "暂无本地版本，可导入 CLIProxyAPI 官方安装包":
    "No local versions. Import an official CLIProxyAPI package to continue.",
  "手动导入不会联网校验 GitHub checksums，请只选择可信的官方安装包。":
    "Manual imports do not verify GitHub checksums online. Only select trusted official packages.",
  "重置服务": "Reset Service",
  "绑定账号": "Bind Accounts",
  "删除账号": "Delete Accounts",
  "重置 API 服务": "Reset API Service",
  "确认重置": "Confirm Reset",
  "API 服务已开启": "API Service started",
  "API 服务已停止": "API Service stopped",
  "API 服务已重置": "API Service reset",
  "服务配置": "Service Config",
  "端口": "Port",
  "管理密钥": "Admin Key",
  "自动更新": "Auto Update",
  "检测间隔": "Check Interval",
  "已关闭": "Disabled",
  "保存后生效": "Save to apply",
  "保存后关闭": "Save to disable",
  "即将检测": "Checking soon",
  "等待后台检测": "Waiting for background check",
  "距下次检测": "Next check in",
  "自动更新失败": "Auto update failed",
  "请稍后手动检测更新": "Please check for updates manually later",
  "API 密钥": "API Keys",
  "随机重生成": "Regenerate",
  "添加密钥": "Add Key",
  "保存配置": "Save Config",
  "按需下载并运行 CLIProxyAPI，本地服务文件保存在 .codex_switcher。":
    "Download and run CLIProxyAPI on demand. Local service files are stored in .codex_switcher.",
  "安装中": "Installing",
  "失败": "Failed",
  "已取消": "Cancelled",
  "未检测": "Not checked",
  "正在下载": "Downloading",
  "准备中": "Preparing",
  "已安装，当前未启动": "Installed, currently stopped",
  "未安装，首次开启时会下载服务": "Not installed. The service will be downloaded on first start.",
  "正在处理服务包": "Processing service package",
  "下载状态": "Download Status",
  "取消下载": "Cancel Download",
  "添加到账号总览": "Add to Accounts",
  "默认使用第一个密钥添加账号，调用地址：": "The first key is used by default when adding an account. Base URL: ",
  "请输入 API 密钥": "Enter an API key",
  "请先添加一个 API 密钥": "Add an API key first",
  "至少保留一个 API 密钥": "Keep at least one API key",
  "API 服务配置已保存": "API service config saved",
  "请先下载并开启 API 服务": "Download and start the API service first",
  "本地 API 服务": "Local API Service",
  "请选择要绑定的 OAuth 账号": "Select OAuth accounts to bind",
  "请选择要绑定的账号": "Select accounts to bind",
  "请选择要删除的 API 服务账号": "Select API service accounts to delete",
  "认证目录里暂无账号": "No accounts in the auth directory",
  "暂无可绑定账号": "No accounts available to bind",
  "没有匹配的账号": "No matching accounts",
  "自定义服务": "Custom Service",
  "API 服务已更新并重新启动": "API service updated and restarted",
  "API 服务更新已安装": "API service update installed",
  "下载已取消": "Download cancelled",
  "服务运行中不能修改端口或密钥，请先停止服务。":
    "Ports and keys cannot be changed while the service is running. Stop the service first.",
  "绑定账号到 API 服务": "Bind Accounts to API Service",
  "选择 OAuth 账号后会转换为 CPA 格式，并写入 API 服务的认证目录。":
    "Selected OAuth accounts are converted to CPA format and written to the API service auth directory.",
  "OAuth 账号会写入认证目录，API Key 账号会写入 CLIProxyAPI 上游配置。":
    "OAuth accounts are written to the auth directory, while API Key accounts are written to the CLIProxyAPI upstream config.",
  "本次确认会先清空 API 服务中的现有账号，再写入所选账号。OAuth 账号会写入认证目录，API Key 账号会写入 CLIProxyAPI 上游配置。":
    "This confirmation first clears existing accounts in the API service, then writes the selected accounts. OAuth accounts go to the auth directory, and API Key accounts go to the CLIProxyAPI upstream config.",
  "已选": "Selected",
  "确认绑定": "Confirm Bind",
  "删除 API 服务账号": "Delete API Service Accounts",
  "这里从认证目录 JSON 内容解析邮箱匹配账号，删除会移除对应 CPA 认证文件。":
    "Accounts are matched by email parsed from auth-directory JSON. Deleting removes the matching CPA auth file.",
  "删除会移除对应 OAuth 认证文件或由本应用管理的 API Key 上游配置。":
    "Deleting removes the matching OAuth auth file or the API Key upstream config managed by this app.",
  "CPA 认证账号": "CPA Auth Account",
  "确认删除": "Confirm Delete",
  "编辑账号顺序": "Edit Account Order",
  "拖动列表项调整顺序，保存后会写入自定义顺序。":
    "Drag list items to adjust order. Saving writes the custom order.",
  "个账号": "accounts",
  "按住拖动排序": "Hold and drag to reorder",
  "保存排序": "Save Order",
  "还没有备份文件": "No backup files yet",
  "先备份一次会话数据，之后就可以从这里只恢复会话。":
    "Back up session data once, then sessions can be restored from here.",
  "Codex 会话不可见": "Codex Sessions Not Visible",
  "检测到 Codex 已切换到": "Detected that Codex switched to",
  "由于官方机制，这类切换后原有会话可能不会自动显示，正在自动修复会话可见性。":
    "Because of Codex behavior, existing sessions may not appear after this kind of switch. Session visibility is being repaired automatically.",
  "修复进度": "Repair Progress",
  "修复已完成": "Repair Complete",
  "修复失败": "Repair Failed",
  "可用": "Available",
  "已使用": "Used",
  "未知": "Unknown",
  "时间未知": "Unknown time",
  "使用": "Used",
  "可用至": "Available until",
  "从未使用": "Never used",
  "额度刷新失败": "Quota refresh failed",
  "额度异常": "Quota Error",
  "双击复制邮箱": "Double-click to copy email",
  "更新信息": "Update Info",
  "最新版本": "Latest Version",
  "匹配平台": "Platform",
  "上次检测": "Last Check",
  "本地目录": "Local Directories",
  "服务目录": "Service Directory",
  "运行时": "Runtime",
  "工作区": "Workspace",
  "配置文件": "Config File",
  "认证目录": "Auth Directory",
  "浏览器登录，自动带回授权结果": "Sign in with browser and return the authorization result automatically.",
  "账号接入": "Account Setup",
  "浏览器授权流程": "Browser Flow",
  "选择一种方式，把账号接到 Codex Switcher": "Choose how to connect an account to Codex Switcher",
  "推荐使用浏览器授权；如果已经有本地 token、JSON 或 API Key，也可以直接导入。":
    "Browser authorization is recommended. Existing local token, JSON, or API Key data can also be imported directly.",
  "生成并打开授权页": "Generate and Open Auth Page",
  "继续打开授权页": "Open Auth Page Again",
  "复制链接": "Copy Link",
  "手动完成": "Manual Completion",
  "完成接入": "Complete",
  "重试保存": "Retry Save",
  "从本地文件导入": "Import from local file",
  "获取本地账号": "Import local account",
  "账号名称": "Account Name",
  "例如：主力账号": "e.g. Primary account",
  "例如：本地 codex 代理": "e.g. Local Codex proxy",
  "示例：": "Example: ",
  "供应商": "Provider",
  "导出 JSON": "Export JSON",
  "批量导出 JSON": "Batch Export JSON",
  "导出格式": "Export Format",
  "预览": "Preview",
  "隐藏预览": "Hide Preview",
  "复制": "Copy",
  "下载": "Download",
  "恢复会话数据": "Restore Session Data",
  "只恢复会话": "Restore Sessions Only",
  "立即备份": "Backup Now",
  "正在备份": "Backing Up",
  "正在备份数据": "Backing Up Data",
  "正在备份会话数据": "Backing Up Session Data",
  "正在启动备份任务...": "Starting backup task...",
  "正在备份...": "Backing up...",
  "正在备份会话文件...": "Backing up session files...",
  "正在恢复完整备份": "Restoring Full Backup",
  "正在恢复会话数据": "Restoring Session Data",
  "正在准备恢复账号、设置与会话数据...": "Preparing to restore accounts, settings, and sessions...",
  "正在准备恢复 Codex 会话数据...": "Preparing to restore Codex session data...",
  "重置次数明细": "Reset Credit Details",
  "重置次数": "Reset Credit",
  "可用重置次数明细": "Available reset credit details",
  "当前有": "currently has",
  "次可用": "available",
  "发放": "Granted",
  "暂无重置次数明细，请先刷新额度": "No reset credit details. Refresh quota first.",
  "按账号执行，实际消耗记录由服务端决定":
    "Runs at the account level; the server decides which reset credit is consumed.",
  "重置记录": "Reset History",
  "额度操作": "Quota Actions",
  "查看预约重置倒计时和历史执行结果": "View scheduled reset countdowns and execution history",
  "活动预约": "Active Schedules",
  "预约重置": "Schedule Reset",
  "立即重置": "Reset Now",
  "保存预约": "Save Schedule",
  "保存修改": "Save Changes",
  "取消预约": "Cancel Schedule",
  "修改预约时间": "Edit Schedule Time",
  "修改以下账号的预约时间": "Edit the scheduled reset time for this account",
  "确认取消该预约？": "Cancel this scheduled reset?",
  "已预约": "Scheduled",
  "执行中": "Running",
  "已完成": "Completed",
  "未执行": "Missed",
  "暂无活动预约": "No active schedules",
  "重置日志": "Reset Logs",
  "暂无重置日志": "No reset logs",
  "删除日志": "Delete Log",
  "清空日志": "Clear Logs",
  "确认删除该条重置日志？": "Delete this reset log?",
  "确认清空全部重置日志？此操作不可恢复。":
    "Clear all reset logs? This action cannot be undone.",
  "选择重置时间": "Choose reset time",
  "设置预约重置时间": "Schedule Reset Time",
  "将为以下账号预约一次重置": "Schedule one reset for this account",
  "请选择有效的预约时间": "Choose a valid scheduled time",
  "预约时间必须晚于当前时间": "The scheduled time must be later than now",
  "预约已开始执行或已无法修改": "The schedule has started or can no longer be edited",
  "该账号已有预约重置": "This account already has a scheduled reset",
  "预约重置已保存": "Scheduled reset saved",
  "预约时间已更新": "Scheduled time updated",
  "预约已取消": "Schedule cancelled",
  "预约重置完成": "Scheduled reset completed",
  "预约重置失败": "Scheduled reset failed",
  "保存重置记录失败": "Failed to save reset history",
  "保存预约重置失败": "Failed to save scheduled reset",
  "取消预约失败": "Failed to cancel the schedule",
  "保存预约修改失败": "Failed to update the schedule",
  "删除重置日志失败": "Failed to delete the reset log",
  "重置日志已删除": "Reset log deleted",
  "清空重置日志失败": "Failed to clear reset logs",
  "重置日志已清空": "Reset logs cleared",
  "加载重置记录失败": "Failed to load reset history",
  "加载重置记录失败：{error}": "Failed to load reset history: {error}",
  "{account} 预约重置失败：{error}":
    "Scheduled reset failed for {account}: {error}",
  "；保存日志失败：{error}": "; failed to save log: {error}",
  "{account} 已重置，但保存日志失败：{error}":
    "Reset completed for {account}, but saving the log failed: {error}",
  "{account} 预约重置完成，但刷新额度失败：{error}":
    "Scheduled reset completed for {account}, but refreshing quota failed: {error}",
  "{account} 预约重置完成": "Scheduled reset completed for {account}",
  "执行预约任务失败：{error}": "Failed to execute scheduled reset: {error}",
  "获取重置次数明细失败：{error}": "Failed to load reset credit details: {error}",
  "重置额度失败：{error}": "Failed to reset quota: {error}",
  "额度已重置，但保存日志失败：{error}":
    "Quota was reset, but saving the log failed: {error}",
  "额度已重置，但刷新额度失败：{error}":
    "Quota was reset, but refreshing quota failed: {error}",
  "找回会话显示": "Recover Session Visibility",
  "会话恢复": "Session Recovery",
  "立即找回": "Recover Now",
  "轻量同步": "Light Sync",
  "完整重建": "Full Rebuild",
  "本地 Codex 管理工具": "Local Codex Management Tool",
  "把账号切换、会话维护、使用统计和本地 API 服务收在一个干净的桌面工作台里。":
    "A clean desktop workspace for account switching, session maintenance, usage analytics, and local API service management.",
  "作者主页": "Author Profile",
  "访问 vs2pk0 的 GitHub 主页": "Open vs2pk0's GitHub profile",
  "开源仓库": "Open Source Repository",
  "查看源码、版本和发布记录": "View source code, versions, and releases",
  "赞助支持": "Sponsor",
  "查看支付宝、微信和 Binance 收款码": "View Alipay, WeChat, and Binance QR codes",
  "问题反馈": "Feedback",
  "提交 Issue 或改进建议": "Submit an issue or suggestion",
  "检查更新": "Check Updates",
  "正在检查更新": "Checking for Updates",
  "查看最新版本并下载安装包": "View the latest version and download installers",
  "当前已是最新版本": "You are up to date",
  "发现新版本": "New version available",
  "检查更新失败": "Update check failed",
  "前往下载": "Go to Download",
  "在线更新": "Update Now",
  "稍后再说": "Later",
  "打开 Releases": "Open Releases",
  "可直接在应用内下载安装包。": "You can download the installer directly in the app.",
  "当前平台没有可用的在线安装包，请前往 GitHub Releases 下载。":
    "No online installer is available for this platform. Download it from GitHub Releases.",
  "暂时无法获取最新版本信息": "The latest version information is temporarily unavailable",
  "可以前往 GitHub Releases 手动查看。": "You can check GitHub Releases manually.",
  "正在下载应用更新": "Downloading App Update",
  "更新安装包下载完成": "Update Installer Downloaded",
  "更新下载失败": "Update Download Failed",
  "更新安装包": "Update Installer",
  "正在获取最新版本": "Getting the latest version",
  "正在连接下载服务器": "Connecting to the download server",
  "等待安装": "Ready to install",
  "安装包已保存，请打开安装包并按提示完成更新。":
    "The installer has been saved. Open it and follow the prompts to complete the update.",
  "重新下载": "Download Again",
  "打开安装包": "Open Installer",
  "取消下载失败": "Failed to cancel download",
  "安装包已打开，请按安装程序完成更新。":
    "The installer is open. Follow its prompts to complete the update.",
  "打开安装包失败": "Failed to open installer",
  "当前实例": "Current Instance",
  "未归属项目": "Unassigned Project",
  "本机全部": "All Local",
  "搜索会话标题": "Search session titles",
  "搜索会话内容": "Search session content",
  "取消全选": "Clear Selection",
  "全选回收站": "Select Trash",
  "会话列表": "Session List",
  "回收站": "Trash",
  "恢复会话": "Restore Sessions",
  "移入回收站": "Move to Trash",
  "未命名会话": "Untitled Session",
  "打开文件夹": "Open Folder",
  "查看会话内容": "View Session Content",
  "会话内容": "Session Content",
  "轮对话": "turns",
  "已加载": "Loaded",
  "个附件": "attachments",
  "搜索已加载的对话内容": "Search loaded conversation content",
  "Markdown 预览": "Markdown Preview",
  "原始文本": "Raw Text",
  "切换所有消息的 Markdown 预览和原始文本": "Switch all messages between Markdown preview and raw text",
  "图片附件": "Image Attachment",
  "支持删除单条消息或完整对话轮次；会话仍在生成时将禁止删除。所有删除都会先自动备份。":
    "Delete one message or a complete turn. Deletion is blocked while the session is active, and every deletion is backed up first.",
  "删除以完整对话轮次为单位；会话仍在生成时将禁止删除。所有删除都会先自动备份。":
    "Deletion applies to a complete turn. Deletion is blocked while the session is active, and every deletion is backed up first.",
  "对话轮次": "Turn",
  "条技术记录已折叠": "technical records collapsed",
  "用户": "User",
  "助手": "Assistant",
  "文件": "File",
  "加载图片预览": "Load image preview",
  "正在加载图片": "Loading image",
  "在文件夹中显示": "Show in Folder",
  "源文件不可用": "Source file unavailable",
  "加载更多对话": "Load More Turns",
  "加载更早对话": "Load Earlier Turns",
  "没有匹配的已加载内容": "No matching loaded content",
  "此会话暂无可显示的消息": "This session has no displayable messages",
  "删除本轮": "Delete Turn",
  "复制消息": "Copy Message",
  "消息已复制": "Message copied",
  "复制消息失败": "Failed to copy message",
  "无法访问系统剪贴板": "Cannot access the system clipboard",
  "删除这条消息": "Delete This Message",
  "删除技能调用": "Delete Skill Call",
  "将删除这次技能调用及其折叠的技能说明。操作前会自动备份会话，可在本窗口撤销。":
    "This removes the skill call and its collapsed skill instructions. The session is backed up first and can be restored here.",
  "将只删除这条消息及其重复历史记录。操作前会自动备份会话，可在本窗口撤销。":
    "This removes only this message and its duplicate history records. The session is backed up first and can be restored here.",
  "消息已删除": "Message deleted",
  "删除消息失败": "Failed to delete message",
  "查看技能说明": "View skill instructions",
  "技能说明": "Skill Instructions",
  "删除这轮对话": "Delete This Turn",
  "当前对话尚未结束，不能删除": "This turn is still active and cannot be deleted",
  "将删除这一轮中的用户消息、回复和附件引用。操作前会自动备份会话，可在本窗口撤销。":
    "This removes the user message, replies, and attachment references in this turn. The session is backed up first and can be restored here.",
  "已删除这轮对话": "Turn deleted",
  "已释放空间": "space released",
  "撤销上次删除": "Undo Last Deletion",
  "将从自动备份恢复删除前的完整会话内容，当前文件也会先创建回滚备份。":
    "The full session will be restored from the automatic backup. The current file will also be backed up before restoration.",
  "确认恢复": "Restore",
  "会话内容已恢复": "Session content restored",
  "读取会话内容失败": "Failed to read session content",
  "读取更多会话内容失败": "Failed to load more session content",
  "会话分页游标没有继续前进": "The session pagination cursor did not advance",
  "加载附件失败": "Failed to load attachment",
  "打开附件失败": "Failed to open attachment",
  "删除会话内容失败": "Failed to delete session content",
  "恢复会话内容失败": "Failed to restore session content",
  "复制到其他会话": "Copy to Another Session",
  "复制到其他目录": "Copy to Another Folder",
  "创建会话副本": "Create Session Copy",
  "修改会话名称": "Rename Session",
  "修改工作目录": "Change Working Directory",
  "选择工作目录失败": "Failed to select working directory",
  "工作目录不能为空": "Working directory cannot be empty",
  "工作目录": "Working Directory",
  "选择目录": "Choose Folder",
  "目标工作目录": "Target Working Directory",
  "请选择副本要归属的工作目录": "Choose the working directory for the copied session",
  "从已有项目选择": "Choose an Existing Project",
  "选择其他目录": "Choose Another Folder",
  "请选择已有的工作目录": "Choose an existing working directory",
  "工作目录已修改": "Working directory updated",
  "工作目录已保存，但部分索引同步失败": "The working directory was saved, but some indexes could not be synchronized",
  "修改工作目录失败": "Failed to change working directory",
  "修改后，此会话将在新目录分组中显示，并在下次继续会话时使用该工作目录。历史记录中的旧目录不会被改写，修改前会自动备份会话文件。":
    "After the change, this session appears under the new folder group and uses that working directory when resumed. Historical directory records remain unchanged, and the session file is backed up first.",
  "如果目标会话正在 Codex 中运行，请先关闭该会话再修改。":
    "If the target session is running in Codex, close it before changing the directory.",
  "复制会话数据": "Copy Session Data",
  "源会话": "Source Session",
  "新会话名称": "New Session Name",
  "新会话会添加到当前列表，已有会话数量和内容不会减少。":
    "The new session is added to the current list. Existing sessions and their content are preserved.",
  "复制会创建一个全新的独立会话，不会覆盖或修改源会话及其他已有会话；新会话会保留原工作目录，并自动重建索引及重启 Codex。":
    "Copying creates a new independent session without overwriting or changing the source or any existing session. The copy keeps the original working directory, rebuilds its indexes, and restarts Codex automatically.",
  "目标会话": "Target Session",
  "目录名称": "Folder Name",
  "请选择目标会话": "Select a target session",
  "请选择新建的空会话": "Select the newly created empty session",
  "确认复制": "Copy and Replace",
  "会话名称": "Session Name",
  "会话名称不能为空": "Session name cannot be empty",
  "输入新的会话名称": "Enter a new session name",
  "请先新建一个空会话并刷新列表": "Create an empty session first, then refresh the list",
  "会话数据已复制并重建历史，Codex 已自动重启": "Session data copied and its history projection rebuilt. Codex restarted automatically.",
  "已新增一个独立会话副本，原有会话均未修改，Codex 已自动重启":
    "A new independent session copy was added. Existing sessions were not changed, and Codex restarted automatically.",
  "副本会显示在目标目录分组中，并与该目录已有会话共存；源会话及其他已有会话不会被修改。":
    "The copy appears in the target folder alongside its existing sessions. The source and all other sessions remain unchanged.",
  "已复制到目标目录，并作为独立会话与现有会话共存，Codex 已自动重启":
    "The session was copied to the target folder as an independent session alongside existing sessions. Codex restarted automatically.",
  "会话已复制，但部分索引同步失败": "The session was copied, but some indexes could not be synchronized",
  "复制会话失败": "Failed to copy session",
  "会话名称已修改": "Session name updated",
  "名称已保存，但部分索引同步失败": "The name was saved, but some indexes could not be synchronized",
  "修改会话名称失败": "Failed to rename session",
  "复制后会保留目标会话的新身份，但目标会话现有内容将被源会话历史覆盖；源会话不会改变，并会自动备份目标会话、重建历史索引及重启 Codex。":
    "The target session keeps its new identity, but its current content will be replaced by the source history. The source stays unchanged; the target is backed up, its history index rebuilt, and Codex restarted automatically.",
  "已删除": "Deleted",
  "没有匹配的会话": "No matching sessions",
  "还没有可显示的会话": "No sessions to show",
  "换个关键词试试，或清空搜索后重新刷新。": "Try another keyword, or clear the search and refresh.",
  "可以先刷新本机会话；如果是切号后看不到旧会话，使用修复可见性重新挂回列表。":
    "Refresh local sessions first. If old sessions disappear after switching accounts, use visibility repair to attach them back.",
  "刷新会话": "Refresh Sessions",
  "修复可见性": "Repair Visibility",
  "从备份恢复": "Restore from Backup",
  "回收站为空": "Trash is Empty",
  "被移入回收站的会话会显示在这里，恢复后会回到原来的会话路径。":
    "Sessions moved to trash appear here and return to their original path after restore.",
  "本机还没有可切换账号": "No switchable accounts yet",
  "先放进一个 Codex 登录态": "Add a Codex login state first",
  "导入 OAuth Token / JSON，或添加 API Key。保存后这里会显示账号卡片，之后就可以一键切换并写回本机 Codex 配置。":
    "Import an OAuth token / JSON, or add an API Key. After saving, account cards appear here and can be switched back into the local Codex config.",
  "导入 Token / JSON": "Import Token / JSON",
  "添加 API Key": "Add API Key",
  "账号添加流程": "Account add flow",
  "粘贴凭据": "Paste Credentials",
  "保存账号": "Save Account",
  "切换 Codex": "Switch Codex",
  "绑定 OAuth 账号": "Bind OAuth Account",
  "API Key 账号绑定 OAuth 后，切换时会同时写入 OAuth Token 与 API Key 配置，便于修复会话身份。":
    "After binding OAuth to an API Key account, switching writes both OAuth Token and API Key config, making session identity repair easier.",
  "不绑定 OAuth": "Do not bind OAuth",
  "切换时仅写入 API Key 配置": "Only write API Key config when switching",
  "暂无可绑定的 OAuth 账号": "No OAuth accounts available",
};

const ru: Record<string, string> = {
  "中文": "Китайский",
  "繁體中文（台灣）": "Традиционный китайский (Тайвань)",
  "English": "Английский",
  "Русский": "Русский",
  "Codex Switcher": "Codex Switcher",
  "管理 OAuth 与 API Key 登录态，并写回本机 Codex 配置。":
    "Управляйте сессиями OAuth и API Key и записывайте их в локальную конфигурацию Codex.",
  "管理 OAuth 与 API Key 登录态，并写回本机 Codex 配置":
    "Управляйте сессиями OAuth и API Key и записывайте их в локальную конфигурацию Codex",
  "账号总览": "Аккаунты",
  "会话管理": "Сессии",
  "使用统计": "Статистика",
  "API 服务": "API-сервис",
  "设置": "Настройки",
  "关于": "О программе",
  "全部": "Все",
  "当前：": "Текущий:",
  "异常": "Проблемные",
  "异常账号": "Проблемные аккаунты",
  "有效账号": "Действительные",
  "当前": "Текущий",
  "读取当前账号": "Определить текущий",
  "已隐藏": "Скрыто",
  "隐身模式": "Режим инкогнито",
  "隐身模式：不会进入 API 服务和 OpenCodex": "Режим инкогнито: аккаунт исключен из выбора API-сервиса и OpenCodex",
  "隐身账号不会出现在 API 服务和 OpenCodex 的账号选择列表中":
    "Аккаунты в режиме инкогнито исключены из списков выбора API-сервиса и OpenCodex",
  "已导入 OpenCodex": "Импортирован в OpenCodex",
  "隐私": "Приватность",
  "徽章样式": "Стиль бейджа",
  "徽章图标样式": "Стиль значков бейджа",
  "{name} 的徽章样式": "Стиль бейджа {name}",
  "30 套视觉方案": "30 визуальных вариантов",
  "经典": "Классика",
  "战术印章": "Тактическая печать",
  "霓虹裂变": "Неоновое деление",
  "琥珀闪电": "Янтарная молния",
  "轻核紫钻": "Аметистовое ядро",
  "星翼火箭": "Звездная ракета",
  "晶格盾章": "Решетчатый щит",
  "赤羽火焰": "Алое пламя",
  "黑曜轻翼": "Обсидиановое крыло",
  "鎏金王冠": "Золотая корона",
  "量子矩阵": "Квантовая матрица",
  "等离子刃": "Плазменный клинок",
  "虚空黑曜": "Пустотный обсидиан",
  "离子脉冲": "Ионный импульс",
  "超新星核": "Ядро сверхновой",
  "焰龙熔铸": "Огненная ковка",
  "赛博棱镜": "Киберпризма",
  "泰坦重甲": "Броня титана",
  "极光镭射": "Аврора-лазер",
  "陨星冲击": "Удар метеора",
  "黑金裁决": "Черно-золотой вердикт",
  "冰川蓝焰": "Ледяное пламя",
  "矩阵绿潮": "Зеленая матрица",
  "日冕爆燃": "Солнечная вспышка",
  "月蚀银弧": "Лунная дуга",
  "裂隙紫电": "Фиолетовый разлом",
  "巅峰红芯": "Пиковое ядро",
  "欧米伽环": "Кольцо Омега",
  "顶点蓝晶": "Вершинный кристалл",
  "零点推进": "Нулевой импульс",
  "添加账号": "Добавить аккаунт",
  "收起侧边栏": "Свернуть боковую панель",
  "展开侧边栏": "Развернуть боковую панель",
  "全选": "Выбрать все",
  "筛选邮箱 / 昵称": "Фильтр email / имя",
  "筛选邮箱 / 昵称 / 标签": "Фильтр email / имя / тег",
  "无数据": "Нет данных",
  "按创建时间": "По времени создания",
  "按周配额": "По недельной квоте",
  "按5小时配额": "По квоте 5 часов",
  "按额度恢复倒计时": "По таймеру восстановления квоты",
  "按标签": "По тегу",
  "按周配额重置时间": "По сбросу недельной квоты",
  "按5小时配额重置时间": "По сбросу квоты 5 часов",
  "按订阅有效期": "По сроку подписки",
  "自定义顺序": "Свой порядок",
  "编辑排序": "Изменить порядок",
  "倒序": "Убыв.",
  "正序": "Возр.",
  "每页": "На странице",
  "卡片": "Карточки",
  "紧凑": "Компактно",
  "表格": "Таблица",
  "切换视图": "Переключить вид",
  "卡片视图": "Карточки",
  "紧凑视图": "Компактный вид",
  "表格视图": "Таблица",
  "绑定到 API 服务": "Привязать к API",
  "加入了 API 服务": "Добавлен в API-сервис",
  "标签": "Тег",
  "选择或输入标签": "Выберите или введите теги",
  "批量导出": "Экспорт",
  "批量导入": "Импорт",
  "当前账号": "Текущий аккаунт",
  "当前页": "Текущая страница",
  "已选择": "Выбрано",
  "清空选择": "Очистить выбор",
  "共": "Всего",
  "条": "шт.",
  "条/页": "шт./стр.",
  "数据": "Данные",
  "应用目录": "Папка приложения",
  "账号 JSON": "JSON аккаунтов",
  "账号目录": "Папка аккаунтов",
  "会话目录": "Папка сессий",
  "统计目录": "Папка статистики",
  "配置目录": "Папка конфигурации",
  "设置 JSON": "JSON настроек",
  "Codex 目录": "Папка Codex",
  "备份目录": "Папка резервных копий",
  "语言": "Язык",
  "侧边栏": "Боковая панель",
  "刷新": "Обновление",
  "监控额度": "Мониторинг квоты",
  "额度自动刷新": "Автообновление квоты",
  "当前账号刷新": "Обновление текущего аккаунта",
  "等待刷新倒计时": "Таймер обновления",
  "显示 GPT 5.3 Codex Spark 额度": "Показывать квоты GPT 5.3 Codex Spark",
  "保存": "Сохранить",
  "外观": "Внешний вид",
  "每行固定账号数": "Аккаунтов в строке",
  "每页账号数": "Аккаунтов на странице",
  "配置": "Конфигурация",
  "推送": "Уведомления",
  "推送设置": "Настройки уведомлений",
  "返回设置": "Назад к настройкам",
  "个渠道": "каналов",
  "立即推送": "Отправить сейчас",
  "定时推送": "Отправка по расписанию",
  "推送周期": "Интервал отправки",
  "主动刷新额度": "Обновлять квоту перед отправкой",
  "下次推送": "Следующая отправка",
  "未启用": "Отключено",
  "账号规则": "Правила аккаунтов",
  "启用": "Включено",
  "账号": "Аккаунт",
  "类型 / 绑定": "Тип / Привязка",
  "额度": "Квота",
  "Token 过期": "Токен истек",
  "推送渠道": "Каналы уведомлений",
  "绑定": "Привязан к",
  "全部启用渠道": "Все включенные каналы",
  "添加渠道": "Добавить канал",
  "渠道昵称": "Название канала",
  "测试": "Тест",
  "企业 ID": "ID организации",
  "推送日志": "Журнал уведомлений",
  "最近": "Последние",
  "条记录": "записей",
  "清空": "Очистить",
  "触发": "Запуск",
  "账号 / 事件": "Аккаунт / Событие",
  "渠道": "Канал",
  "结果": "Результат",
  "内容": "Содержимое",
  "响应": "Ответ",
  "成功": "Успешно",
  "手动": "Вручную",
  "定时": "По расписанию",
  "还没有推送渠道": "Каналы еще не добавлены",
  "还没有推送日志": "Журнал уведомлений пуст",
  "确认删除这个推送渠道？": "Удалить этот канал уведомлений?",
  "确认清空全部推送日志？": "Очистить весь журнал уведомлений?",
  "加载推送设置失败": "Не удалось загрузить настройки уведомлений",
  "推送设置尚未加载完成": "Настройки уведомлений еще не загружены",
  "请先开启定时推送": "Сначала включите отправку по расписанию",
  "请至少启用一个账号推送规则": "Включите хотя бы одно правило аккаунта",
  "请至少启用一个推送渠道": "Включите хотя бы один канал уведомлений",
  "已启用账号必须选择至少一个推送事件": "Для каждого включенного аккаунта выберите хотя бы одно событие",
  "账号规则选择的渠道均未启用": "Все каналы, выбранные в правиле аккаунта, отключены",
  "请填写": "Заполните",
  "推送设置已保存": "Настройки уведомлений сохранены",
  "保存推送设置失败": "Не удалось сохранить настройки уведомлений",
  "保存的渠道不存在": "Сохраненный канал не был возвращен",
  "保存的规则不存在": "Сохраненное правило не было возвращено",
  "测试推送成功": "Тестовое уведомление отправлено",
  "测试推送失败": "Не удалось отправить тестовое уведомление",
  "立即推送失败": "Не удалось отправить уведомление",
  "加载推送日志失败": "Не удалось загрузить журнал уведомлений",
  "推送日志已清空": "Журнал уведомлений очищен",
  "清空推送日志失败": "Не удалось очистить журнал уведомлений",
  "通过规则将账号状态推送到一个或多个渠道": "Отправляйте статус аккаунтов в один или несколько каналов по правилам",
  "自动执行": "Автоматизация",
  "执行全部": "Запустить все",
  "保存设置": "Сохранить настройки",
  "推送规则": "Правила уведомлений",
  "已启用规则": "Включенные правила",
  "今日发送": "Отправлено сегодня",
  "规则列表": "Список правил",
  "规则": "Правила",
  "账号仅在规则编辑时选择，不在页面中铺开显示": "Аккаунты выбираются только при редактировании правила",
  "新增规则": "Новое правило",
  "还没有推送规则": "Правила уведомлений еще не созданы",
  "规则名称": "Название правила",
  "例如：重要账号额度预警": "Например: предупреждение о квоте важного аккаунта",
  "账号范围": "Аккаунты",
  "触发条件": "Условия запуска",
  "下次检查": "Следующая проверка",
  "个触发器": "условий",
  "执行规则": "Запустить правило",
  "确认删除这个推送规则？": "Удалить это правило уведомлений?",
  "可同时启用多个条件，任一条件满足即可推送": "Можно включить несколько условий; достаточно выполнения любого",
  "每隔": "Каждые",
  "剩余额度低于": "Остаток квоты ниже",
  "剩余时间不超过": "Оставшееся время не более",
  "检测到 Token 已失效或接口返回过期状态时推送": "Уведомлять, если токен недействителен или API сообщает об истечении",
  "读取额度发生错误或账号状态异常时推送": "Уведомлять при ошибке чтения квоты или состоянии аккаунта",
  "结果排序": "Сортировка результатов",
  "账号列表顺序": "Порядок аккаунтов",
  "剩余额度升序": "Квота по возрастанию",
  "订阅到期升序": "Срок подписки по возрастанию",
  "Token 到期升序": "Срок токена по возрастанию",
  "重复提醒间隔": "Интервал повторов",
  "推送前主动刷新": "Обновить перед отправкой",
  "关闭时直接读取定时任务保存的账号状态": "Если выключено, используется статус из планового обновления",
  "规则预览": "Предпросмотр правила",
  "未选择渠道": "Каналы не выбраны",
  "选择一个或多个渠道": "Выберите один или несколько каналов",
  "未选择账号": "Аккаунты не выбраны",
  "选择要监控的账号": "Выберите аккаунты для мониторинга",
  "暂无可选账号": "Нет доступных аккаунтов",
  "新增第一条规则": "Создать первое правило",
  "渠道管理": "Управление каналами",
  "规则可同时选择多个已启用渠道": "Правило может использовать несколько включенных каналов",
  "搜索渠道": "Поиск каналов",
  "没有匹配的渠道": "Подходящие каналы не найдены",
  "未设置渠道昵称": "Название канала не задано",
  "保存渠道": "Сохранить канал",
  "渠道已保存": "Канал сохранен",
  "规则 / 触发": "Правило / Запуск",
  "匹配账号 / 事件": "Аккаунты / События",
  "渠道测试": "Тест канала",
  "已隐藏账号": "Аккаунт скрыт",
  "隐私模式下已隐藏推送内容": "Содержимое уведомления скрыто в режиме конфиденциальности",
  "额度低于": "Квота ниже",
  "订阅剩余": "Остаток подписки",
  "剩余": "осталось",
  "Token 已过期": "Токен истек",
  "账号异常": "Ошибка аккаунта",
  "未设置触发条件": "Условия не заданы",
  "账号状态提醒": "Статус аккаунтов",
  "副本": "Копия",
  "请选择至少一个账号": "Выберите хотя бы один аккаунт",
  "规则包含已失效账号，请重新选择": "Правило содержит недоступные аккаунты. Выберите аккаунты заново",
  "请选择至少一个推送渠道": "Выберите хотя бы один канал уведомлений",
  "请至少启用一个触发条件": "Включите хотя бы одно условие",
  "规则选择的渠道均未启用": "Все выбранные для правила каналы отключены",
  "请先新增并启用推送规则": "Сначала создайте и включите правило",
  "匹配账号": "Подходящие аккаунты",
  "当前没有账号满足条件": "Сейчас ни один аккаунт не соответствует условиям",
  "定时状态": "Плановый статус",
  "额度不足": "Мало квоты",
  "订阅临期": "Подписка истекает",
  "Token 临期": "Токен истекает",
  "自动": "Автоматически",
  "天窗口": "дн. окно",
  "小时窗口": "ч. окно",
  "分钟窗口": "мин. окно",
  "项目": "Проекты",
  "会话": "Сессии",
  "磁盘占用": "На диске",
  "项目名称": "Название проекта",
  "占用": "Размер",
  "更新时间": "Обновлено",
  "展开": "Развернуть",
  "收起": "Свернуть",
  "展开会话": "Развернуть сессии",
  "条会话": "сессий",
  "项": "элементов",
  "总 Tokens": "Всего токенов",
  "总占用": "Общий размер",
  "编辑 auth.json": "Изменить auth.json",
  "编辑 config.toml": "Изменить config.toml",
  "修复会话模型": "Исправить модели сессий",
  "一键修复切号会话": "Исправить сессии после смены аккаунта",
  "一键修复": "Исправить всё",
  "当前切号会话无需修复": "Сессии после смены аккаунта уже совместимы",
  "切号会话一键修复完成": "Сессии после смены аккаунта исправлены",
  "一键修复切号会话失败": "Не удалось исправить сессии после смены аккаунта",
  "清理加密引用": "удалено зашифрованных ссылок",
  "无论从正常账号切到 API 服务账号，还是再切回来，都会按当前账号双向修复 Provider 模型前缀、线程模型与推理强度，并清理上一个账号遗留的 cmp/rs 加密引用。只移除无法跨账号复用的推理和压缩元数据，用户消息、助手回复与工具记录都会保留；修复前会自动备份，期间 ChatGPT/Codex 会自动重启。":
    "При переключении с официального аккаунта на аккаунт API-сервиса и обратно квалифицированные имена моделей исправляются в обоих направлениях, настройки модели и глубины рассуждений приводятся к текущему аккаунту, а зашифрованные ссылки cmp/rs от предыдущего аккаунта удаляются. Удаляются только метаданные рассуждений и сжатия, которые нельзя использовать с другим аккаунтом; сообщения пользователя, ответы помощника и записи инструментов сохраняются. Сначала создаётся резервная копия, затем ChatGPT/Codex автоматически перезапускается.",
  "将按当前账号清理本地会话中残留的旧 Provider 模型前缀，重置显式模型与推理强度，并同步 Provider。修复前会备份会话文件和数据库，不会删除会话内容。修复期间 ChatGPT/Codex 会自动重启。":
    "Устаревшие префиксы поставщика будут удалены из моделей локальных сессий, явные настройки модели и глубины рассуждений сброшены, а поставщик синхронизирован с текущим аккаунтом. Перед исправлением файлы и базы сессий будут сохранены; содержимое сессий не удаляется. ChatGPT/Codex автоматически перезапустится.",
  "开始修复": "Начать исправление",
  "当前会话模型配置无需修复": "Настройки моделей сессий уже совместимы",
  "会话模型修复完成": "Модели сессий исправлены",
  "修复会话模型失败": "Не удалось исправить модели сессий",
  "个会话文件": "файлов сессий",
  "直接查看并编辑本机 Codex 配置文件，保存前会检查格式并自动备份原文件。":
    "Просматривайте и редактируйте локальные файлы конфигурации Codex. Перед сохранением формат проверяется, а исходный файл резервируется.",
  "文件已存在": "Файл существует",
  "文件不存在，保存时会创建": "Файл не существует и будет создан при сохранении",
  "auth.json 包含登录令牌等敏感信息，请勿截图、复制或分享给他人。":
    "auth.json содержит конфиденциальные данные, включая токены входа. Не делайте снимки экрана, не копируйте и не передавайте файл.",
  "请输入配置内容": "Введите содержимое конфигурации",
  "查找配置内容": "Поиск в конфигурации",
  "上一个匹配": "Предыдущее совпадение",
  "下一个匹配": "Следующее совпадение",
  "关闭查找": "Закрыть поиск",
  "API Key 模型列表": "Модели API Key",
  "目标账号": "Целевой аккаунт",
  "当前默认": "Текущая модель",
  "筛选模型名称": "Фильтр моделей",
  "重新获取列表": "Обновить список",
  "获取模型列表": "Получить модели",
  "正在读取本机 Codex 配置并核对当前 API Key…": "Чтение локальной конфигурации Codex и проверка активного API Key…",
  "当前 API Key 与目标账号匹配，可以设置模型。": "Активный API Key соответствует этому аккаунту. Модель можно изменить.",
  "当前 Codex 配置不是此 API Key，请先切换到该账号后再设置模型。":
    "Активная конфигурация Codex не использует этот API Key. Перед выбором модели переключитесь на этот аккаунт.",
  "读取本机 Codex 配置失败": "Не удалось прочитать локальную конфигурацию Codex",
  "重新检测": "Проверить снова",
  "提供方": "Поставщик",
  "5.6 配置": "Конфигурация 5.6",
  "没有匹配的模型": "Совпадающих моделей нет",
  "点击“获取模型列表”从当前 API 服务读取可用模型": "Нажмите «Получить модели», чтобы загрузить модели этого API-сервиса",
  "API 服务没有返回可用模型": "API-сервис не вернул доступных моделей",
  "获取模型列表失败": "Не удалось получить список моделей",
  "请选择默认模型": "Выберите модель по умолчанию",
  "默认模型已保存并写入 config.toml": "Модель по умолчанию сохранена в config.toml",
  "保存默认模型失败": "Не удалось сохранить модель по умолчанию",
  "选择 5.6 系列模型后，将同步写入 Responses 模式与兼容配置，并移除旧 WebSocket 配置。":
    "При выборе модели 5.6 будут записаны настройки совместимости Responses и удалены старые настройки WebSocket.",
  "设为默认模型": "Сделать моделью по умолчанию",
  "格式化并检查": "Форматировать и проверить",
  "重新加载": "Перезагрузить",
  "保存文件": "Сохранить файл",
  "重置 config.toml": "Сбросить config.toml",
  "删除 config.toml": "Удалить config.toml",
  "确认将本机 Codex 目录下的 config.toml 恢复为内置基础配置？当前文件会先自动备份。":
    "Восстановить встроенную базовую конфигурацию config.toml в локальной папке Codex? Текущий файл сначала будет сохранён в резервную копию.",
  "恢复基础配置": "Восстановить базовую конфигурацию",
  "已将 config.toml 恢复为基础配置": "config.toml восстановлен до базовой конфигурации",
  "确认永久删除本机 Codex 目录下的 config.toml？此操作只删除当前配置文件，不会自动恢复基础配置。":
    "Безвозвратно удалить config.toml из локальной папки Codex? Будет удалён только текущий файл, базовая конфигурация автоматически не восстановится.",
  "已删除 config.toml": "config.toml удалён",
  "config.toml 不存在，无需删除": "config.toml не существует; удалять нечего",
  "备份": "Резервная копия",
  "打开备份目录": "Открыть папку",
  "恢复": "Восстановить",
  "删除": "Удалить",
  "开始恢复": "Начать восстановление",
  "恢复完整备份": "Восстановить полную копию",
  "还没有备份": "Резервных копий пока нет",
  "点击“手动备份”会把账号、设置、统计缓存、费用规则与所有 Codex 会话记录打包成 ZIP。":
    "Нажмите «Создать копию», чтобы упаковать аккаунты, настройки, кэш статистики, правила цен и все сессии Codex в ZIP.",
  "确认删除这个备份文件？": "Удалить этот файл резервной копии?",
  "手动备份": "Создать копию",
  "分钟": "минут",
  "小时": "ч",
  "天": "дн.",
  "万": "10 тыс.",
  "亿": "100 млн",
  "已过期": "Истекло",
  "已记录": "Записано",
  "等待刷新": "Ожидание",
  "个": "шт.",
  "重置会删除本机 Codex 目录下的 config.toml，适合切换配置异常时恢复默认配置。":
    "Сброс удалит config.toml в локальной папке Codex и вернет настройки по умолчанию при ошибках переключения.",
  "取消": "Отмена",
  "确认": "Подтвердить",
  "关闭": "Закрыть",
  "导入": "Импорт",
  "添加": "Добавить",
  "编辑": "Изменить",
  "表单": "Форма",
  "JSON": "JSON",
  "Base URL": "Base URL",
  "API Key": "API Key",
  "接入新账号": "Добавить аккаунт",
  "重新授权账号": "Повторная авторизация",
  "编辑账号": "Изменить аккаунт",
  "编辑 API Key": "Изменить API Key",
  "编辑 OAuth 账号": "Изменить OAuth-аккаунт",
  "切换": "Переключить",
  "置顶账号": "Закрепить",
  "取消置顶": "Открепить",
  "绑定手机": "Привязать телефон",
  "未命名账号": "Безымянный аккаунт",
  "未设置": "Не задано",
  "未保存": "Не сохранено",
  "邮箱": "Email",
  "订阅": "Подписка",
  "订阅信息": "Информация о подписке",
  "配额状态": "Статус квоты",
  "配额 / 余额": "Квота / Баланс",
  "余额": "Баланс",
  "密钥额度": "Квота ключа",
  "套餐剩余": "Остаток тарифа",
  "刷新余额": "Обновить баланс",
  "点击刷新余额": "Нажмите, чтобы обновить баланс",
  "余额获取中": "Загрузка баланса",
  "余额获取失败": "Не удалось загрузить баланс",
  "余额获取失败，点击重试": "Не удалось загрузить баланс. Нажмите, чтобы повторить",
  "余额已刷新": "Баланс обновлён",
  "需确认": "Нужно подтверждение",
  "HTTP 地址需点击确认后查询余额": "Для HTTP требуется подтверждение. Нажмите, чтобы запросить баланс",
  "HTTP 连接安全提示": "Предупреждение о безопасности HTTP",
  "该中转站使用未加密的 HTTP 连接。查询余额时会通过此连接发送 API Key，同一网络中的设备或代理可能读取它。是否仅在本次运行期间允许？":
    "Этот ретранслятор использует незашифрованный HTTP. При запросе баланса API Key передаётся по этому соединению и может быть прочитан устройствами или прокси в той же сети. Разрешить только на время этого запуска?",
  "仅本次运行允许": "Разрешить только для этого запуска",
  "账号配置已变化，请重新点击余额并确认": "Настройки аккаунта изменились. Нажмите на баланс и подтвердите снова",
  "获取失败": "Недоступно",
  "更新于": "Обновлено",
  "不限量": "Без ограничений",
  "未获得订阅信息": "Нет данных подписки",
  "OAuth 登录": "Вход OAuth",
  "Token 登录": "Вход по токену",
  "用户 ID": "ID пользователя",
  "账号 ID": "ID аккаунта",
  "周配额": "Недельная квота",
  "可用重置次数": "Доступные сбросы",
  "刷新额度": "Обновить квоту",
  "刷新全部额度": "Обновить все квоты",
  "请先在设置中开启额度监控": "Сначала включите мониторинг квот в настройках",
  "没有可刷新的 OAuth 账号": "Нет OAuth-аккаунтов для обновления",
  "重置额度": "Сбросить квоту",
  "导出": "Экспорт",
  "重新授权": "Авторизовать заново",
  "官网地址": "Официальный сайт",
  "额度概览": "Обзор квоты",
  "自动同步": "Автосинхронизация",
  "长周期": "Длинный период",
  "短周期": "Короткий период",
  "订阅状态": "Подписка",
  "密钥状态": "Статус ключа",
  "Token 可用": "Токен доступен",
  "Token 失效": "Токен недействителен",
  "Token 失效，请重新登录或更换绑定账号": "Токен недействителен. Авторизуйтесь заново или смените привязанный аккаунт.",
  "请重新登录或更换绑定账号": "Авторизуйтесь заново или смените привязанный аккаунт",
  "绑定 OAuth": "Привязать OAuth",
  "未绑定": "Не привязан",
  "消耗看板": "Панель расхода",
  "从本机会话记录汇总 Tokens、缓存复用和预估费用":
    "Сводка токенов, повторного использования кэша и примерной стоимости по локальным сессиям.",
  "当天": "Сегодня",
  "昨天": "Вчера",
  "前天": "Позавчера",
  "当月": "Этот месяц",
  "上月": "Прошлый месяц",
  "请选择有效的开始和结束时间，日期必须真实存在且开始时间不能晚于结束时间。":
    "Выберите корректные даты начала и окончания. Начало не должно быть позже окончания.",
  "本地 Codex 消耗": "Локальный расход Codex",
  "总请求数": "Запросы",
  "预估费用": "Оценка стоимости",
  "新增输入 Tokens": "Новые входные токены",
  "输出 Tokens": "Выходные токены",
  "缓存写入": "Запись кэша",
  "缓存复用": "Повтор кэша",
  "输入": "Ввод",
  "输出": "Вывод",
  "平均": "сред.",
  "复用占比": "Доля повтора",
  "趋势": "Тренд",
  "时段消耗曲线": "Расход по периодам",
  "暂无趋势数据": "Нет данных тренда",
  "累计 Token 数": "Всего токенов",
  "峰值 Token 数": "Пиковые токены",
  "当前连续天数": "Текущая серия",
  "最长连续天数": "Лучшая серия",
  "Token 活动": "Активность токенов",
  "凌晨": "Ночь",
  "清晨": "Раннее утро",
  "上午": "Утро",
  "午后": "День",
  "傍晚": "Вечер",
  "夜间": "Поздний вечер",
  "每日": "По дням",
  "每周": "По неделям",
  "累计": "Всего",
  "时段": "Период",
  "当日": "День",
  "暂无活动数据": "Нет данных активности",
  "调用流水": "Вызовы",
  "来源汇总": "Источники",
  "模型用量": "Модели",
  "调用记录": "Журнал вызовов",
  "时间": "Время",
  "来源": "Источник",
  "计费模型": "Модель тарификации",
  "费用": "Стоимость",
  "状态": "Статус",
  "暂无使用记录": "Нет записей использования",
  "请求数": "Запросы",
  "Tokens": "Токены",
  "成功率": "Успешность",
  "平均延迟": "Средняя задержка",
  "模型": "Модель",
  "单次均价": "Средняя стоимость",
  "本期概览": "Обзор периода",
  "主要来源": "Главный источник",
  "主要模型": "Главная модель",
  "费用倍率": "Множитель стоимости",
  "请求模型": "Модель запроса",
  "返回模型": "Модель ответа",
  "来源分布": "Распределение источников",
  "模型分布": "Распределение моделей",
  "暂无来源数据": "Нет данных источников",
  "暂无模型用量": "Нет данных моделей",
  "请求明细": "Журнал запросов",
  "Provider 统计": "Статистика провайдеров",
  "模型统计": "Статистика моделей",
  "费用规则": "Цены",
  "维护 Codex 统计使用的模型单价和倍率": "Настройка цен моделей и множителей для статистики Codex",
  "Codex 计费口径": "Правила тарификации Codex",
  "设置统计倍率与模型识别来源": "Настройте множитель статистики и источник модели",
  "应用": "Приложение",
  "默认倍率": "Множитель",
  "计费模式": "Режим тарификации",
  "模型单价（每百万 Tokens）": "Цены моделей (за 1M токенов)",
  "条规则": "правил",
  "恢复默认": "Вернуть по умолчанию",
  "显示名称": "Отображаемое имя",
  "输入单价": "Цена ввода",
  "输出单价": "Цена вывода",
  "操作": "Действия",
  "暂无模型单价": "Нет цен моделей",
  "确定删除这条模型单价？": "Удалить это правило цены?",
  "编辑模型单价": "Изменить цену модели",
  "添加模型单价": "Добавить цену модели",
  "模型 ID": "ID модели",
  "输入单价 / 1M": "Цена ввода / 1M",
  "输出单价 / 1M": "Цена вывода / 1M",
  "缓存复用 / 1M": "Повтор кэша / 1M",
  "缓存写入 / 1M": "Запись кэша / 1M",
  "例如 gpt-5-codex": "например gpt-5-codex",
  "例如 GPT-5 Codex": "например GPT-5 Codex",
  "恢复内置 GPT/Codex 单价会覆盖当前 pricing.json，确定继续？":
    "Встроенные цены GPT/Codex перезапишут pricing.json. Продолжить?",
  "服务状态": "Статус сервиса",
  "当前版本": "Текущая версия",
  "访问地址": "Адрес",
  "未安装": "Не установлен",
  "已安装": "Установлен",
  "运行中": "Работает",
  "开启服务": "Запустить",
  "下载并开启": "Скачать и запустить",
  "停止服务": "Остановить",
  "检测更新": "Проверить обновления",
  "下载更新": "Скачать обновление",
  "版本管理": "Управление версиями",
  "API 服务版本管理": "Версии API-сервиса",
  "本地版本": "Локальные версии",
  "更新或导入成功后默认切换到本机最高版本，并保留当前版本和最多 2 个旧版本。":
    "После обновления или импорта самой новой локальной версии назначается статус текущей; сохраняются текущая и до двух старых версий.",
  "导入本地包": "Импортировать пакет",
  "GitHub 更新接口当前限流，可打开“版本管理”导入本地安装包":
    "Запросы обновлений GitHub временно ограничены. Откройте управление версиями и импортируйте локальный пакет.",
  "CLIProxyAPI 安装包": "Пакет CLIProxyAPI",
  "导入本地版本包": "Импорт локальной версии",
  "手动导入用于 GitHub 限流或网络不可用的情况，不会通过 GitHub checksums 在线校验。请只导入可信的 CLIProxyAPI 官方安装包。导入成功后会自动使用本机最高版本，并仅保留当前版本和 2 个旧版本。":
    "Ручной импорт предназначен для случаев ограничения GitHub или отсутствия сети и не проверяет GitHub checksums онлайн. Импортируйте только доверенные официальные пакеты CLIProxyAPI. После импорта самой новой локальной версии назначается статус текущей; сохраняются текущая и две старые версии.",
  "确认导入": "Подтвердить импорт",
  "版本包已导入，已切换到本机最新版本":
    "Пакет импортирован, выбрана самая новая локальная версия.",
  "导入失败：{error}": "Ошибка импорта: {error}",
  "选择版本包失败：{error}": "Не удалось выбрать пакет версии: {error}",
  "切换到 v{version}": "Переключиться на v{version}",
  "API 服务正在运行，切换时会自动重启；如果新版本启动失败，将恢复当前版本。是否继续？":
    "API-сервис работает и будет перезапущен при переключении. Если выбранная версия не запустится, будет восстановлена текущая версия. Продолжить?",
  "切换后，该版本会成为下次启动 API 服务时使用的版本。是否继续？":
    "Эта версия будет использована при следующем запуске API-сервиса. Продолжить?",
  "已切换到 v{version}": "Выполнено переключение на v{version}",
  "切换失败：{error}": "Ошибка переключения: {error}",
  "删除 v{version}": "Удалить v{version}",
  "将删除这个本地运行时版本，当前使用的版本不会受影响。是否继续？":
    "Эта локальная версия среды выполнения будет удалена. Текущая версия не изменится. Продолжить?",
  "已删除 v{version}": "Удалена v{version}",
  "删除失败：{error}": "Ошибка удаления: {error}",
  "旧版本清理尚未完成：当前有 {count} 个旧版本，最多应保留 {limit} 个。请稍后重试删除。":
    "Очистка старых версий не завершена: осталось {count}, допустимо не более {limit}. Повторите удаление позже.",
  "版本": "Версия",
  "平台": "Платформа",
  "导入时间": "Время импорта",
  "包文件": "Файл пакета",
  "状态与操作": "Статус и действия",
  "设为当前": "Сделать текущей",
  "平台不兼容": "Несовместимая платформа",
  "不可删除": "Удаление запрещено",
  "暂无本地版本，可导入 CLIProxyAPI 官方安装包":
    "Локальных версий нет. Импортируйте официальный пакет CLIProxyAPI.",
  "手动导入不会联网校验 GitHub checksums，请只选择可信的官方安装包。":
    "При ручном импорте GitHub checksums не проверяются. Используйте только доверенные официальные пакеты.",
  "重置服务": "Сброс сервиса",
  "绑定账号": "Привязать аккаунты",
  "删除账号": "Удалить аккаунты",
  "重置 API 服务": "Сброс API-сервиса",
  "确认重置": "Подтвердить сброс",
  "API 服务已开启": "API-сервис запущен",
  "API 服务已停止": "API-сервис остановлен",
  "API 服务已重置": "API-сервис сброшен",
  "服务配置": "Конфигурация сервиса",
  "端口": "Порт",
  "管理密钥": "Ключ администратора",
  "自动更新": "Автообновление",
  "检测间隔": "Интервал проверки",
  "已关闭": "Отключено",
  "保存后生效": "Сохраните для применения",
  "保存后关闭": "Сохраните для отключения",
  "即将检测": "Скоро проверка",
  "等待后台检测": "Ожидание фоновой проверки",
  "距下次检测": "До следующей проверки",
  "自动更新失败": "Ошибка автообновления",
  "请稍后手动检测更新": "Проверьте обновления вручную позже",
  "API 密钥": "API-ключи",
  "随机重生成": "Сгенерировать",
  "添加密钥": "Добавить ключ",
  "保存配置": "Сохранить",
  "按需下载并运行 CLIProxyAPI，本地服务文件保存在 .codex_switcher。":
    "CLIProxyAPI скачивается и запускается по необходимости. Файлы сервиса хранятся в .codex_switcher.",
  "安装中": "Установка",
  "失败": "Ошибка",
  "已取消": "Отменено",
  "未检测": "Не проверено",
  "正在下载": "Загрузка",
  "准备中": "Подготовка",
  "已安装，当前未启动": "Установлен, сейчас остановлен",
  "未安装，首次开启时会下载服务": "Не установлен. При первом запуске сервис будет скачан.",
  "正在处理服务包": "Обработка пакета сервиса",
  "下载状态": "Статус загрузки",
  "取消下载": "Отменить загрузку",
  "添加到账号总览": "Добавить в аккаунты",
  "默认使用第一个密钥添加账号，调用地址：": "Первый ключ используется при добавлении аккаунта. Base URL: ",
  "请输入 API 密钥": "Введите API-ключ",
  "请先添加一个 API 密钥": "Сначала добавьте API-ключ",
  "至少保留一个 API 密钥": "Оставьте хотя бы один API-ключ",
  "API 服务配置已保存": "Конфигурация API-сервиса сохранена",
  "请先下载并开启 API 服务": "Сначала скачайте и запустите API-сервис",
  "本地 API 服务": "Локальный API-сервис",
  "请选择要绑定的 OAuth 账号": "Выберите OAuth-аккаунты для привязки",
  "请选择要绑定的账号": "Выберите аккаунты для привязки",
  "请选择要删除的 API 服务账号": "Выберите аккаунты API-сервиса для удаления",
  "认证目录里暂无账号": "В каталоге авторизации пока нет аккаунтов",
  "暂无可绑定账号": "Нет доступных аккаунтов для привязки",
  "没有匹配的账号": "Совпадающих аккаунтов нет",
  "自定义服务": "Пользовательский сервис",
  "API 服务已更新并重新启动": "API-сервис обновлен и перезапущен",
  "API 服务更新已安装": "Обновление API-сервиса установлено",
  "下载已取消": "Загрузка отменена",
  "服务运行中不能修改端口或密钥，请先停止服务。":
    "Порт и ключи нельзя менять во время работы сервиса. Сначала остановите сервис.",
  "绑定账号到 API 服务": "Привязать аккаунты к API-сервису",
  "选择 OAuth 账号后会转换为 CPA 格式，并写入 API 服务的认证目录。":
    "Выбранные OAuth-аккаунты будут преобразованы в формат CPA и записаны в каталог авторизации API-сервиса.",
  "OAuth 账号会写入认证目录，API Key 账号会写入 CLIProxyAPI 上游配置。":
    "OAuth-аккаунты записываются в каталог авторизации, а API Key — в конфигурацию upstream CLIProxyAPI.",
  "本次确认会先清空 API 服务中的现有账号，再写入所选账号。OAuth 账号会写入认证目录，API Key 账号会写入 CLIProxyAPI 上游配置。":
    "При подтверждении текущие аккаунты API-сервиса будут очищены, затем будут записаны выбранные аккаунты. OAuth-аккаунты попадут в каталог авторизации, API Key — в upstream-конфигурацию CLIProxyAPI.",
  "已选": "Выбрано",
  "确认绑定": "Подтвердить привязку",
  "删除 API 服务账号": "Удалить аккаунты API-сервиса",
  "这里从认证目录 JSON 内容解析邮箱匹配账号，删除会移除对应 CPA 认证文件。":
    "Аккаунты сопоставляются по email из JSON в каталоге авторизации. Удаление уберет соответствующий CPA-файл.",
  "删除会移除对应 OAuth 认证文件或由本应用管理的 API Key 上游配置。":
    "Удаление уберет соответствующий OAuth-файл или конфигурацию upstream API Key, управляемую приложением.",
  "CPA 认证账号": "CPA-аккаунт авторизации",
  "确认删除": "Подтвердить удаление",
  "编辑账号顺序": "Изменить порядок аккаунтов",
  "拖动列表项调整顺序，保存后会写入自定义顺序。":
    "Перетащите элементы списка, чтобы изменить порядок. После сохранения он будет записан как пользовательский.",
  "个账号": "аккаунтов",
  "按住拖动排序": "Удерживайте и перетащите для сортировки",
  "保存排序": "Сохранить порядок",
  "恢复会话数据": "Восстановить данные сессий",
  "只恢复会话": "Восстановить только сессии",
  "还没有备份文件": "Файлов резервных копий пока нет",
  "先备份一次会话数据，之后就可以从这里只恢复会话。":
    "Сначала создайте резервную копию сессий, затем их можно будет восстановить отсюда.",
  "立即备份": "Создать копию",
  "Codex 会话不可见": "Сессии Codex не видны",
  "检测到 Codex 已切换到": "Обнаружено переключение Codex на",
  "由于官方机制，这类切换后原有会话可能不会自动显示，正在自动修复会话可见性。":
    "Из-за поведения Codex существующие сессии могут не отображаться после такого переключения. Видимость сессий восстанавливается автоматически.",
  "修复进度": "Прогресс восстановления",
  "修复已完成": "Восстановление завершено",
  "修复失败": "Восстановление не удалось",
  "可用": "Доступно",
  "已使用": "Использовано",
  "未知": "Неизвестно",
  "时间未知": "Время неизвестно",
  "使用": "Использовано",
  "可用至": "Доступно до",
  "从未使用": "Не использовалось",
  "额度刷新失败": "Не удалось обновить квоту",
  "额度异常": "Ошибка квоты",
  "双击复制邮箱": "Двойной клик копирует email",
  "更新信息": "Обновления",
  "最新版本": "Последняя версия",
  "匹配平台": "Платформа",
  "上次检测": "Последняя проверка",
  "本地目录": "Локальные папки",
  "服务目录": "Папка сервиса",
  "运行时": "Среда выполнения",
  "工作区": "Рабочая папка",
  "配置文件": "Файл конфигурации",
  "认证目录": "Папка авторизации",
  "浏览器登录，自动带回授权结果": "Войдите в браузере, результат вернется автоматически.",
  "账号接入": "Подключение аккаунта",
  "浏览器授权流程": "Авторизация в браузере",
  "选择一种方式，把账号接到 Codex Switcher": "Выберите способ подключения аккаунта к Codex Switcher",
  "推荐使用浏览器授权；如果已经有本地 token、JSON 或 API Key，也可以直接导入。":
    "Рекомендуется авторизация в браузере. Локальный token, JSON или API Key также можно импортировать напрямую.",
  "生成并打开授权页": "Создать и открыть страницу",
  "继续打开授权页": "Открыть страницу снова",
  "复制链接": "Копировать ссылку",
  "手动完成": "Завершить вручную",
  "完成接入": "Готово",
  "重试保存": "Повторить сохранение",
  "从本地文件导入": "Импорт из локального файла",
  "获取本地账号": "Импорт локального аккаунта",
  "账号名称": "Имя аккаунта",
  "例如：主力账号": "например: основной аккаунт",
  "例如：本地 codex 代理": "например: локальный Codex-прокси",
  "示例：": "Пример: ",
  "供应商": "Провайдер",
  "导出 JSON": "Экспорт JSON",
  "批量导出 JSON": "Массовый экспорт JSON",
  "导出格式": "Формат экспорта",
  "预览": "Предпросмотр",
  "隐藏预览": "Скрыть предпросмотр",
  "复制": "Копировать",
  "下载": "Скачать",
  "正在备份": "Создание копии",
  "正在备份数据": "Создание копии данных",
  "正在备份会话数据": "Создание копии сессий",
  "正在启动备份任务...": "Запуск задачи резервного копирования...",
  "正在备份...": "Создание копии...",
  "正在备份会话文件...": "Создание копии файлов сессий...",
  "正在恢复完整备份": "Восстановление полной копии",
  "正在恢复会话数据": "Восстановление сессий",
  "正在准备恢复账号、设置与会话数据...": "Подготовка восстановления аккаунтов, настроек и сессий...",
  "正在准备恢复 Codex 会话数据...": "Подготовка восстановления данных сессий Codex...",
  "重置次数明细": "Сведения о сбросах",
  "重置次数": "Сбросы",
  "可用重置次数明细": "Сведения о доступных сбросах",
  "当前有": "сейчас доступно",
  "次可用": "доступно",
  "发放": "Выдано",
  "暂无重置次数明细，请先刷新额度": "Нет сведений о сбросах. Сначала обновите квоту.",
  "按账号执行，实际消耗记录由服务端决定":
    "Операция выполняется для аккаунта; конкретный сброс выбирает сервер.",
  "重置记录": "История сбросов",
  "额度操作": "Операции с квотой",
  "查看预约重置倒计时和历史执行结果":
    "Просмотр обратного отсчёта и истории выполнения сбросов",
  "活动预约": "Активные сбросы",
  "预约重置": "Запланировать сброс",
  "立即重置": "Сбросить сейчас",
  "保存预约": "Сохранить сброс",
  "取消预约": "Отменить сброс",
  "修改预约时间": "Изменить время сброса",
  "修改以下账号的预约时间": "Изменить время запланированного сброса этого аккаунта",
  "保存修改": "Сохранить изменения",
  "确认取消该预约？": "Отменить этот запланированный сброс?",
  "已预约": "Запланировано",
  "执行中": "Выполняется",
  "已完成": "Завершено",
  "未执行": "Не выполнено",
  "暂无活动预约": "Нет активных сбросов",
  "重置日志": "Журналы сброса",
  "暂无重置日志": "Нет журналов сброса",
  "删除日志": "Удалить журнал",
  "清空日志": "Очистить журналы",
  "确认删除该条重置日志？": "Удалить эту запись журнала сброса?",
  "确认清空全部重置日志？此操作不可恢复。":
    "Очистить все журналы сброса? Это действие нельзя отменить.",
  "选择重置时间": "Выбрать время сброса",
  "设置预约重置时间": "Запланировать время сброса",
  "将为以下账号预约一次重置": "Запланировать сброс для этого аккаунта",
  "请选择有效的预约时间": "Выберите допустимое время сброса",
  "预约时间必须晚于当前时间": "Время сброса должно быть позже текущего",
  "预约已开始执行或已无法修改": "Сброс уже выполняется или его нельзя изменить",
  "该账号已有预约重置": "Для этого аккаунта уже запланирован сброс",
  "预约重置已保存": "Запланированный сброс сохранён",
  "预约时间已更新": "Время сброса обновлено",
  "预约已取消": "Запланированный сброс отменён",
  "预约重置完成": "Запланированный сброс выполнен",
  "预约重置失败": "Не удалось выполнить запланированный сброс",
  "保存重置记录失败": "Не удалось сохранить историю сбросов",
  "保存预约重置失败": "Не удалось сохранить запланированный сброс",
  "取消预约失败": "Не удалось отменить запланированный сброс",
  "保存预约修改失败": "Не удалось изменить время сброса",
  "删除重置日志失败": "Не удалось удалить журнал сброса",
  "重置日志已删除": "Журнал сброса удалён",
  "清空重置日志失败": "Не удалось очистить журналы сброса",
  "重置日志已清空": "Журналы сброса очищены",
  "加载重置记录失败": "Не удалось загрузить историю сбросов",
  "加载重置记录失败：{error}": "Не удалось загрузить историю сбросов: {error}",
  "{account} 预约重置失败：{error}":
    "Не удалось выполнить запланированный сброс для {account}: {error}",
  "；保存日志失败：{error}": "; не удалось сохранить журнал: {error}",
  "{account} 已重置，但保存日志失败：{error}":
    "Сброс для {account} выполнен, но не удалось сохранить журнал: {error}",
  "{account} 预约重置完成，但刷新额度失败：{error}":
    "Запланированный сброс для {account} выполнен, но не удалось обновить квоту: {error}",
  "{account} 预约重置完成": "Запланированный сброс для {account} выполнен",
  "执行预约任务失败：{error}": "Не удалось выполнить запланированную задачу сброса: {error}",
  "获取重置次数明细失败：{error}": "Не удалось загрузить сведения о сбросах: {error}",
  "重置额度失败：{error}": "Не удалось сбросить квоту: {error}",
  "额度已重置，但保存日志失败：{error}":
    "Квота сброшена, но не удалось сохранить журнал: {error}",
  "额度已重置，但刷新额度失败：{error}":
    "Квота сброшена, но не удалось обновить её: {error}",
  "会话恢复": "Восстановление сессий",
  "本地 Codex 管理工具": "Локальный инструмент управления Codex",
  "把账号切换、会话维护、使用统计和本地 API 服务收在一个干净的桌面工作台里。":
    "Удобное рабочее место для переключения аккаунтов, обслуживания сессий, статистики и локального API-сервиса.",
  "作者主页": "Профиль автора",
  "访问 vs2pk0 的 GitHub 主页": "Открыть профиль vs2pk0 на GitHub",
  "开源仓库": "Открытый репозиторий",
  "查看源码、版本和发布记录": "Посмотреть код, версии и релизы",
  "赞助支持": "Поддержать проект",
  "查看支付宝、微信和 Binance 收款码": "Открыть QR-коды Alipay, WeChat и Binance",
  "问题反馈": "Обратная связь",
  "提交 Issue 或改进建议": "Создать issue или предложить улучшение",
  "检查更新": "Проверить обновления",
  "正在检查更新": "Проверка обновлений",
  "查看最新版本并下载安装包": "Посмотреть последнюю версию и скачать установщик",
  "当前已是最新版本": "У вас последняя версия",
  "发现新版本": "Доступна новая версия",
  "检查更新失败": "Не удалось проверить обновления",
  "前往下载": "Перейти к загрузке",
  "在线更新": "Обновить сейчас",
  "稍后再说": "Позже",
  "打开 Releases": "Открыть Releases",
  "可直接在应用内下载安装包。": "Установщик можно загрузить прямо в приложении.",
  "当前平台没有可用的在线安装包，请前往 GitHub Releases 下载。":
    "Для этой платформы нет доступного онлайн-установщика. Загрузите его с GitHub Releases.",
  "暂时无法获取最新版本信息": "Информация о последней версии временно недоступна",
  "可以前往 GitHub Releases 手动查看。": "Версию можно проверить вручную на GitHub Releases.",
  "正在下载应用更新": "Загрузка обновления приложения",
  "更新安装包下载完成": "Установщик обновления загружен",
  "更新下载失败": "Не удалось загрузить обновление",
  "更新安装包": "Установщик обновления",
  "正在获取最新版本": "Получение последней версии",
  "正在连接下载服务器": "Подключение к серверу загрузки",
  "等待安装": "Готово к установке",
  "安装包已保存，请打开安装包并按提示完成更新。":
    "Установщик сохранен. Откройте его и следуйте инструкциям для завершения обновления.",
  "重新下载": "Загрузить снова",
  "打开安装包": "Открыть установщик",
  "取消下载失败": "Не удалось отменить загрузку",
  "安装包已打开，请按安装程序完成更新。":
    "Установщик открыт. Следуйте его инструкциям для завершения обновления.",
  "打开安装包失败": "Не удалось открыть установщик",
  "搜索会话标题": "Поиск по названию сессии",
  "未归属项目": "Проект не назначен",
  "搜索会话内容": "Поиск по содержимому сессии",
  "取消全选": "Снять выделение",
  "全选回收站": "Выбрать корзину",
  "会话列表": "Список сессий",
  "回收站": "Корзина",
  "恢复会话": "Восстановить сессии",
  "移入回收站": "Переместить в корзину",
  "未命名会话": "Безымянная сессия",
  "打开文件夹": "Открыть папку",
  "查看会话内容": "Просмотреть содержимое сессии",
  "会话内容": "Содержимое сессии",
  "轮对话": "ходов",
  "已加载": "Загружено",
  "个附件": "вложений",
  "搜索已加载的对话内容": "Поиск в загруженных сообщениях",
  "Markdown 预览": "Предпросмотр Markdown",
  "原始文本": "Исходный текст",
  "切换所有消息的 Markdown 预览和原始文本": "Переключить все сообщения между Markdown и исходным текстом",
  "图片附件": "Изображение",
  "支持删除单条消息或完整对话轮次；会话仍在生成时将禁止删除。所有删除都会先自动备份。":
    "Можно удалить отдельное сообщение или весь ход. Во время активной сессии удаление запрещено; перед удалением создается резервная копия.",
  "删除以完整对话轮次为单位；会话仍在生成时将禁止删除。所有删除都会先自动备份。":
    "Удаляется весь ход диалога. Во время активной сессии удаление запрещено; перед удалением создается резервная копия.",
  "对话轮次": "Ход",
  "条技术记录已折叠": "технических записей свернуто",
  "用户": "Пользователь",
  "助手": "Ассистент",
  "文件": "Файл",
  "加载图片预览": "Загрузить изображение",
  "正在加载图片": "Загрузка изображения",
  "在文件夹中显示": "Показать в папке",
  "源文件不可用": "Исходный файл недоступен",
  "加载更多对话": "Загрузить еще",
  "加载更早对话": "Загрузить более ранние ходы",
  "没有匹配的已加载内容": "Совпадений в загруженном содержимом нет",
  "此会话暂无可显示的消息": "В этой сессии нет отображаемых сообщений",
  "删除本轮": "Удалить ход",
  "复制消息": "Копировать сообщение",
  "消息已复制": "Сообщение скопировано",
  "复制消息失败": "Не удалось скопировать сообщение",
  "无法访问系统剪贴板": "Нет доступа к системному буферу обмена",
  "删除这条消息": "Удалить это сообщение",
  "删除技能调用": "Удалить вызов навыка",
  "将删除这次技能调用及其折叠的技能说明。操作前会自动备份会话，可在本窗口撤销。":
    "Будут удалены вызов навыка и свернутые инструкции. Перед удалением создается резервная копия.",
  "将只删除这条消息及其重复历史记录。操作前会自动备份会话，可在本窗口撤销。":
    "Будут удалены только это сообщение и его дубликаты в истории. Перед удалением создается резервная копия.",
  "消息已删除": "Сообщение удалено",
  "删除消息失败": "Не удалось удалить сообщение",
  "查看技能说明": "Просмотреть инструкции навыка",
  "技能说明": "Инструкции навыка",
  "删除这轮对话": "Удалить этот ход",
  "当前对话尚未结束，不能删除": "Этот ход еще активен и не может быть удален",
  "将删除这一轮中的用户消息、回复和附件引用。操作前会自动备份会话，可在本窗口撤销。":
    "Будут удалены сообщение пользователя, ответы и ссылки на вложения этого хода. Перед удалением создается резервная копия.",
  "已删除这轮对话": "Ход удален",
  "已释放空间": "освобождено",
  "撤销上次删除": "Отменить последнее удаление",
  "将从自动备份恢复删除前的完整会话内容，当前文件也会先创建回滚备份。":
    "Полная сессия будет восстановлена из автоматической копии. Перед восстановлением текущий файл также будет сохранен.",
  "确认恢复": "Восстановить",
  "会话内容已恢复": "Содержимое сессии восстановлено",
  "读取会话内容失败": "Не удалось прочитать содержимое сессии",
  "读取更多会话内容失败": "Не удалось загрузить дополнительные сообщения",
  "会话分页游标没有继续前进": "Курсор пагинации сессии не продвинулся",
  "加载附件失败": "Не удалось загрузить вложение",
  "打开附件失败": "Не удалось открыть вложение",
  "删除会话内容失败": "Не удалось удалить содержимое сессии",
  "恢复会话内容失败": "Не удалось восстановить содержимое сессии",
  "复制到其他会话": "Копировать в другую сессию",
  "复制到其他目录": "Копировать в другую папку",
  "创建会话副本": "Создать копию сессии",
  "修改会话名称": "Переименовать сессию",
  "修改工作目录": "Изменить рабочую папку",
  "选择工作目录失败": "Не удалось выбрать рабочую папку",
  "工作目录不能为空": "Рабочая папка не может быть пустой",
  "工作目录": "Рабочая папка",
  "选择目录": "Выбрать папку",
  "目标工作目录": "Целевая рабочая папка",
  "请选择副本要归属的工作目录": "Выберите рабочую папку для копии сессии",
  "从已有项目选择": "Выбрать существующий проект",
  "选择其他目录": "Выбрать другую папку",
  "请选择已有的工作目录": "Выберите существующую рабочую папку",
  "工作目录已修改": "Рабочая папка изменена",
  "工作目录已保存，但部分索引同步失败": "Рабочая папка сохранена, но часть индексов не синхронизирована",
  "修改工作目录失败": "Не удалось изменить рабочую папку",
  "修改后，此会话将在新目录分组中显示，并在下次继续会话时使用该工作目录。历史记录中的旧目录不会被改写，修改前会自动备份会话文件。":
    "После изменения сессия появится в новой группе и будет использовать выбранную рабочую папку при продолжении. Старые записи не изменяются, а файл сессии предварительно сохраняется.",
  "如果目标会话正在 Codex 中运行，请先关闭该会话再修改。":
    "Если целевая сессия запущена в Codex, закройте ее перед изменением папки.",
  "复制会话数据": "Копировать данные сессии",
  "源会话": "Исходная сессия",
  "新会话名称": "Название новой сессии",
  "新会话会添加到当前列表，已有会话数量和内容不会减少。":
    "Новая сессия будет добавлена в текущий список. Существующие сессии и их содержимое сохранятся.",
  "复制会创建一个全新的独立会话，不会覆盖或修改源会话及其他已有会话；新会话会保留原工作目录，并自动重建索引及重启 Codex。":
    "Копирование создаст новую независимую сессию, не перезаписывая исходную или другие существующие сессии. Копия сохранит рабочую папку, перестроит индексы и автоматически перезапустит Codex.",
  "目标会话": "Целевая сессия",
  "目录名称": "Имя папки",
  "请选择目标会话": "Выберите целевую сессию",
  "请选择新建的空会话": "Выберите новую пустую сессию",
  "确认复制": "Копировать и заменить",
  "会话名称": "Название сессии",
  "会话名称不能为空": "Название сессии не может быть пустым",
  "输入新的会话名称": "Введите новое название сессии",
  "请先新建一个空会话并刷新列表": "Сначала создайте пустую сессию и обновите список",
  "会话数据已复制并重建历史，Codex 已自动重启": "Данные сессии скопированы, проекция истории перестроена, Codex автоматически перезапущен.",
  "已新增一个独立会话副本，原有会话均未修改，Codex 已自动重启":
    "Добавлена новая независимая копия сессии. Существующие сессии не изменены, Codex автоматически перезапущен.",
  "副本会显示在目标目录分组中，并与该目录已有会话共存；源会话及其他已有会话不会被修改。":
    "Копия появится в целевой папке рядом с существующими сессиями. Исходная и остальные сессии не изменяются.",
  "已复制到目标目录，并作为独立会话与现有会话共存，Codex 已自动重启":
    "Сессия скопирована в целевую папку как независимая и сохранена рядом с существующими сессиями. Codex автоматически перезапущен.",
  "会话已复制，但部分索引同步失败": "Сессия скопирована, но часть индексов не синхронизирована",
  "复制会话失败": "Не удалось скопировать сессию",
  "会话名称已修改": "Название сессии изменено",
  "名称已保存，但部分索引同步失败": "Название сохранено, но часть индексов не синхронизирована",
  "修改会话名称失败": "Не удалось переименовать сессию",
  "复制后会保留目标会话的新身份，但目标会话现有内容将被源会话历史覆盖；源会话不会改变，并会自动备份目标会话、重建历史索引及重启 Codex。":
    "Целевая сессия сохранит новый идентификатор, но ее содержимое будет заменено историей исходной сессии. Исходная сессия не изменится; целевая будет сохранена, индекс истории перестроен, а Codex автоматически перезапущен.",
  "已删除": "Удалено",
  "没有匹配的会话": "Сессии не найдены",
  "还没有可显示的会话": "Нет сессий для отображения",
  "换个关键词试试，或清空搜索后重新刷新。": "Попробуйте другой запрос или очистите поиск и обновите.",
  "可以先刷新本机会话；如果是切号后看不到旧会话，使用修复可见性重新挂回列表。":
    "Сначала обновите локальные сессии. Если старые сессии не видны после переключения, восстановите видимость.",
  "刷新会话": "Обновить сессии",
  "修复可见性": "Восстановить видимость",
  "从备份恢复": "Восстановить из копии",
  "回收站为空": "Корзина пуста",
  "被移入回收站的会话会显示在这里，恢复后会回到原来的会话路径。":
    "Сессии из корзины отображаются здесь и возвращаются в исходный путь после восстановления.",
  "本机还没有可切换账号": "Пока нет аккаунтов для переключения",
  "先放进一个 Codex 登录态": "Сначала добавьте состояние входа Codex",
  "导入 OAuth Token / JSON，或添加 API Key。保存后这里会显示账号卡片，之后就可以一键切换并写回本机 Codex 配置。":
    "Импортируйте OAuth Token / JSON или добавьте API Key. После сохранения карточки появятся здесь, и их можно будет переключать в локальную конфигурацию Codex.",
  "导入 Token / JSON": "Импорт Token / JSON",
  "添加 API Key": "Добавить API Key",
  "账号添加流程": "Процесс добавления аккаунта",
  "粘贴凭据": "Вставить данные",
  "保存账号": "Сохранить аккаунт",
  "切换 Codex": "Переключить Codex",
  "绑定 OAuth 账号": "Привязать OAuth аккаунт",
  "API Key 账号绑定 OAuth 后，切换时会同时写入 OAuth Token 与 API Key 配置，便于修复会话身份。":
    "После привязки OAuth к аккаунту API Key при переключении будут записываться OAuth Token и API Key, что помогает восстановить идентичность сессии.",
  "不绑定 OAuth": "Не привязывать OAuth",
  "切换时仅写入 API Key 配置": "При переключении записывать только API Key",
  "暂无可绑定的 OAuth 账号": "Нет OAuth аккаунтов для привязки",
};

const dictionary: Record<Exclude<AppLanguage, "zh-CN" | "zh-TW">, Record<string, string>> = {
  en,
  ru,
};
let reverseDictionary: Map<string, string> | null = null;

const simplifiedToTraditional: Record<string, string> = {
  "Token 临期": "Token 臨期",
  "Token 到期升序": "Token 到期遞增",
  "Token 已过期": "Token 已過期",
  "下次检查": "下次檢查",
  "个渠道": "個管道",
  "个触发器": "個觸發條件",
  "个账号": "個帳號",
  "今日发送": "今日傳送",
  "例如：重要账号额度预警": "例如：重要帳號額度預警",
  "保存设置": "儲存設定",
  "全选": "全選",
  "关闭时直接读取定时任务保存的账号状态": "關閉時直接讀取排程任務儲存的帳號狀態",
  "分钟": "分鐘",
  "分钟窗口": "分鐘視窗",
  "剩余": "剩餘",
  "剩余时间不超过": "剩餘時間不超過",
  "剩余额度低于": "剩餘額度低於",
  "剩余额度升序": "剩餘額度遞增",
  "加入了 API 服务": "加入了 API 服務",
  "按额度恢复倒计时": "依額度恢復倒數",
  "按标签": "依標籤",
  "本次确认会先清空 API 服务中的现有账号，再写入所选账号。OAuth 账号会写入认证目录，API Key 账号会写入 CLIProxyAPI 上游配置。":
    "本次確認會先清空 API 服務中的現有帳號，再寫入所選帳號。OAuth 帳號會寫入認證目錄，API Key 帳號會寫入 CLIProxyAPI 上游設定。",
  "副本": "副本",
  "匹配账号": "符合帳號",
  "匹配账号 / 事件": "符合帳號 / 事件",
  "可同时启用多个条件，任一条件满足即可推送": "可同時啟用多個條件，任一條件符合即可推送",
  "启用": "啟用",
  "响应": "回應",
  "天": "天",
  "天窗口": "天視窗",
  "定时": "定時",
  "定时状态": "定時狀態",
  "小时": "小時",
  "小时窗口": "小時視窗",
  "已启用规则": "已啟用規則",
  "已选": "已選",
  "当前没有账号满足条件": "目前沒有帳號符合條件",
  "手动": "手動",
  "执行全部": "執行全部",
  "执行规则": "執行規則",
  "推送前主动刷新": "推送前主動重新整理",
  "推送规则": "推送規則",
  "推送设置尚未加载完成": "推送設定尚未載入完成",
  "操作": "操作",
  "新增第一条规则": "新增第一條規則",
  "新增规则": "新增規則",
  "最近": "最近",
  "未设置触发条件": "未設定觸發條件",
  "未选择渠道": "未選擇管道",
  "未选择账号": "未選擇帳號",
  "检测到 Token 已失效或接口返回过期状态时推送": "偵測到 Token 已失效或 API 回傳過期狀態時推送",
  "每隔": "每隔",
  "测试": "測試",
  "清空": "清空",
  "渠道": "管道",
  "渠道测试": "管道測試",
  "已隐藏账号": "已隱藏帳號",
  "隐私模式下已隐藏推送内容": "隱私模式下已隱藏推送內容",
  "渠道管理": "管道管理",
  "短周期": "短週期",
  "确认删除这个推送规则？": "確定刪除這個推送規則？",
  "筛选邮箱 / 昵称": "篩選信箱 / 暱稱",
  "筛选邮箱 / 昵称 / 标签": "篩選信箱 / 暱稱 / 標籤",
  "标签": "標籤",
  "选择或输入标签": "選擇或輸入標籤",
  "结果": "結果",
  "结果排序": "結果排序",
  "自动执行": "自動執行",
  "自定义服务": "自訂服務",
  "规则 / 触发": "規則 / 觸發",
  "规则列表": "規則清單",
  "规则可同时选择多个已启用渠道": "規則可同時選擇多個已啟用管道",
  "规则名称": "規則名稱",
  "规则选择的渠道均未启用": "規則選擇的管道均未啟用",
  "规则预览": "規則預覽",
  "触发条件": "觸發條件",
  "订阅临期": "訂閱臨期",
  "订阅到期升序": "訂閱到期遞增",
  "订阅剩余": "訂閱剩餘",
  "请先新增并启用推送规则": "請先新增並啟用推送規則",
  "请填写": "請填寫",
  "请至少启用一个触发条件": "請至少啟用一個觸發條件",
  "请选择至少一个推送渠道": "請選擇至少一個推送管道",
  "请选择至少一个账号": "請選擇至少一個帳號",
  "规则包含已失效账号，请重新选择": "規則包含已失效帳號，請重新選擇",
  "读取额度发生错误或账号状态异常时推送": "讀取額度發生錯誤或帳號狀態異常時推送",
  "账号仅在规则编辑时选择，不在页面中铺开显示": "帳號僅在規則編輯時選擇，不在頁面中展開顯示",
  "账号列表顺序": "帳號清單順序",
  "账号异常": "帳號異常",
  "账号状态提醒": "帳號狀態提醒",
  "账号范围": "帳號範圍",
  "还没有推送规则": "尚未新增推送規則",
  "选择一个或多个渠道": "選擇一個或多個管道",
  "选择要监控的账号": "選擇要監控的帳號",
  "通过规则将账号状态推送到一个或多个渠道": "透過規則將帳號狀態推送到一個或多個管道",
  "重复提醒间隔": "重複提醒間隔",
  "长周期": "長週期",
  "额度不足": "額度不足",
  "额度低于": "額度低於",
  "展开": "展開",
  "收起": "收合",
  "展开会话": "展開會話",
  "条会话": "條會話",
  "取消全选": "取消全選",
  "全选回收站": "全選回收站",
  "修复可见性": "修復可見性",
  "从备份恢复": "從備份還原",
  "会话列表": "會話清單",
  "共": "共",
  "刷新会话": "重新整理會話",
  "占用": "占用空間",
  "可以先刷新本机会话；如果是切号后看不到旧会话，使用修复可见性重新挂回列表。":
    "可以先重新整理本機會話；如果切換帳號後看不到舊會話，請使用修復可見性重新掛回清單。",
  "回收站": "資源回收筒",
  "回收站为空": "資源回收筒是空的",
  "已删除": "已刪除",
  "已选择": "已選擇",
  "总 Tokens": "總 Tokens",
  "总占用": "總占用空間",
  "恢复会话": "還原會話",
  "打开文件夹": "開啟資料夾",
  "查看会话内容": "查看會話內容",
  "会话内容": "會話內容",
  "轮对话": "輪對話",
  "已加载": "已載入",
  "个附件": "個附件",
  "搜索已加载的对话内容": "搜尋已載入的對話內容",
  "Markdown 预览": "Markdown 預覽",
  "原始文本": "原始文字",
  "切换所有消息的 Markdown 预览和原始文本": "切換所有訊息的 Markdown 預覽和原始文字",
  "图片附件": "圖片附件",
  "支持删除单条消息或完整对话轮次；会话仍在生成时将禁止删除。所有删除都会先自动备份。":
    "支援刪除單條訊息或完整對話輪次；會話仍在產生時將禁止刪除。所有刪除都會先自動備份。",
  "删除以完整对话轮次为单位；会话仍在生成时将禁止删除。所有删除都会先自动备份。":
    "刪除以完整對話輪次為單位；會話仍在產生時將禁止刪除。所有刪除都會先自動備份。",
  "对话轮次": "對話輪次",
  "条技术记录已折叠": "條技術記錄已摺疊",
  "用户": "使用者",
  "助手": "助手",
  "加载图片预览": "載入圖片預覽",
  "正在加载图片": "正在載入圖片",
  "在文件夹中显示": "在資料夾中顯示",
  "源文件不可用": "來源檔案無法使用",
  "加载更多对话": "載入更多對話",
  "加载更早对话": "載入更早對話",
  "没有匹配的已加载内容": "沒有符合的已載入內容",
  "此会话暂无可显示的消息": "此會話暫無可顯示的訊息",
  "删除本轮": "刪除本輪",
  "复制消息": "複製訊息",
  "消息已复制": "訊息已複製",
  "复制消息失败": "複製訊息失敗",
  "无法访问系统剪贴板": "無法存取系統剪貼簿",
  "删除这条消息": "刪除這條訊息",
  "删除技能调用": "刪除技能呼叫",
  "将删除这次技能调用及其折叠的技能说明。操作前会自动备份会话，可在本窗口撤销。":
    "將刪除這次技能呼叫及其摺疊的技能說明。操作前會自動備份會話，可在本視窗復原。",
  "将只删除这条消息及其重复历史记录。操作前会自动备份会话，可在本窗口撤销。":
    "將只刪除這條訊息及其重複歷史記錄。操作前會自動備份會話，可在本視窗復原。",
  "消息已删除": "訊息已刪除",
  "删除消息失败": "刪除訊息失敗",
  "查看技能说明": "查看技能說明",
  "技能说明": "技能說明",
  "删除这轮对话": "刪除這輪對話",
  "当前对话尚未结束，不能删除": "目前對話尚未結束，不能刪除",
  "将删除这一轮中的用户消息、回复和附件引用。操作前会自动备份会话，可在本窗口撤销。":
    "將刪除這一輪中的使用者訊息、回覆和附件參照。操作前會自動備份會話，可在本視窗復原。",
  "确认删除": "確認刪除",
  "已删除这轮对话": "已刪除這輪對話",
  "已释放空间": "已釋放空間",
  "撤销上次删除": "復原上次刪除",
  "将从自动备份恢复删除前的完整会话内容，当前文件也会先创建回滚备份。":
    "將從自動備份還原刪除前的完整會話內容，目前檔案也會先建立回復備份。",
  "确认恢复": "確認還原",
  "会话内容已恢复": "會話內容已還原",
  "读取会话内容失败": "讀取會話內容失敗",
  "读取更多会话内容失败": "讀取更多會話內容失敗",
  "会话分页游标没有继续前进": "會話分頁游標沒有繼續前進",
  "加载附件失败": "載入附件失敗",
  "打开附件失败": "開啟附件失敗",
  "删除会话内容失败": "刪除會話內容失敗",
  "恢复会话内容失败": "還原會話內容失敗",
  "复制到其他会话": "複製到其他會話",
  "复制到其他目录": "複製到其他目錄",
  "创建会话副本": "建立會話副本",
  "修改会话名称": "修改會話名稱",
  "修改工作目录": "修改工作目錄",
  "选择工作目录失败": "選擇工作目錄失敗",
  "工作目录不能为空": "工作目錄不能為空",
  "工作目录": "工作目錄",
  "选择目录": "選擇目錄",
  "目标工作目录": "目標工作目錄",
  "请选择副本要归属的工作目录": "請選擇副本要歸屬的工作目錄",
  "从已有项目选择": "從現有專案選擇",
  "选择其他目录": "選擇其他目錄",
  "请选择已有的工作目录": "請選擇已有的工作目錄",
  "工作目录已修改": "工作目錄已修改",
  "工作目录已保存，但部分索引同步失败": "工作目錄已儲存，但部分索引同步失敗",
  "修改工作目录失败": "修改工作目錄失敗",
  "修改后，此会话将在新目录分组中显示，并在下次继续会话时使用该工作目录。历史记录中的旧目录不会被改写，修改前会自动备份会话文件。":
    "修改後，此會話將在新目錄分組中顯示，並在下次繼續會話時使用該工作目錄。歷史記錄中的舊目錄不會被改寫，修改前會自動備份會話檔案。",
  "如果目标会话正在 Codex 中运行，请先关闭该会话再修改。":
    "如果目標會話正在 Codex 中執行，請先關閉該會話再修改。",
  "复制会话数据": "複製會話資料",
  "源会话": "來源會話",
  "新会话名称": "新會話名稱",
  "新会话会添加到当前列表，已有会话数量和内容不会减少。":
    "新會話會加入目前清單，既有會話數量和內容不會減少。",
  "复制会创建一个全新的独立会话，不会覆盖或修改源会话及其他已有会话；新会话会保留原工作目录，并自动重建索引及重启 Codex。":
    "複製會建立一個全新的獨立會話，不會覆蓋或修改來源會話及其他既有會話；新會話會保留原工作目錄，並自動重建索引及重新啟動 Codex。",
  "目标会话": "目標會話",
  "目录名称": "目錄名稱",
  "请选择目标会话": "請選擇目標會話",
  "请选择新建的空会话": "請選擇新建的空白會話",
  "确认复制": "確認複製",
  "会话名称": "會話名稱",
  "会话名称不能为空": "會話名稱不能為空",
  "输入新的会话名称": "輸入新的會話名稱",
  "请先新建一个空会话并刷新列表": "請先新建一個空白會話並重新整理清單",
  "会话数据已复制并重建历史，Codex 已自动重启": "會話資料已複製並重建歷史，Codex 已自動重新啟動",
  "已新增一个独立会话副本，原有会话均未修改，Codex 已自动重启":
    "已新增一個獨立會話副本，原有會話均未修改，Codex 已自動重新啟動",
  "副本会显示在目标目录分组中，并与该目录已有会话共存；源会话及其他已有会话不会被修改。":
    "副本會顯示在目標目錄群組中，並與該目錄既有會話共存；來源會話及其他既有會話不會被修改。",
  "已复制到目标目录，并作为独立会话与现有会话共存，Codex 已自动重启":
    "已複製到目標目錄，並作為獨立會話與現有會話共存，Codex 已自動重新啟動",
  "会话已复制，但部分索引同步失败": "會話已複製，但部分索引同步失敗",
  "复制会话失败": "複製會話失敗",
  "会话名称已修改": "會話名稱已修改",
  "名称已保存，但部分索引同步失败": "名稱已儲存，但部分索引同步失敗",
  "修改会话名称失败": "修改會話名稱失敗",
  "复制后会保留目标会话的新身份，但目标会话现有内容将被源会话历史覆盖；源会话不会改变，并会自动备份目标会话、重建历史索引及重启 Codex。":
    "複製後會保留目標會話的新身分，但目標會話現有內容將被來源會話歷史覆蓋；來源會話不會改變，並會自動備份目標會話、重建歷史索引及重新啟動 Codex。",
  "换个关键词试试，或清空搜索后重新刷新。": "請換個關鍵字，或清除搜尋後重新整理。",
  "搜索会话内容": "搜尋會話內容",
  "搜索会话标题": "搜尋會話標題",
  "更新时间": "更新時間",
  "未命名会话": "未命名會話",
  "没有匹配的会话": "沒有符合的會話",
  "磁盘占用": "磁碟占用",
  "移入回收站": "移至資源回收筒",
  "被移入回收站的会话会显示在这里，恢复后会回到原来的会话路径。":
    "移至資源回收筒的會話會顯示在這裡，還原後會回到原本的會話路徑。",
  "还没有可显示的会话": "目前沒有可顯示的會話",
  "项": "項",
  "项目": "專案",
  "项目名称": "專案名稱",
  "推送设置": "推送設定",
  "返回设置": "返回設定",
  "定时推送": "定時推送",
  "推送周期": "推送週期",
  "主动刷新额度": "主動重新整理額度",
  "下次推送": "下次推送",
  "账号规则": "帳號規則",
  "类型 / 绑定": "類型 / 綁定",
  "Token 过期": "Token 過期",
  "推送渠道": "推送管道",
  "全部启用渠道": "全部啟用管道",
  "添加渠道": "新增管道",
  "渠道昵称": "管道暱稱",
  "搜索渠道": "搜尋管道",
  "没有匹配的渠道": "沒有符合的管道",
  "未设置渠道昵称": "尚未設定管道暱稱",
  "保存渠道": "儲存管道",
  "渠道已保存": "管道已儲存",
  "企业 ID": "企業 ID",
  "推送日志": "推送紀錄",
  "条记录": "筆紀錄",
  "账号 / 事件": "帳號 / 事件",
  "内容": "內容",
  "还没有推送渠道": "尚未新增推送管道",
  "还没有推送日志": "尚無推送紀錄",
  "确认删除这个推送渠道？": "確定刪除這個推送管道？",
  "确认清空全部推送日志？": "確定清空全部推送紀錄？",
  "加载推送设置失败": "載入推送設定失敗",
  "请先开启定时推送": "請先開啟定時推送",
  "请至少启用一个账号推送规则": "請至少啟用一個帳號推送規則",
  "请至少启用一个推送渠道": "請至少啟用一個推送管道",
  "已启用账号必须选择至少一个推送事件": "已啟用帳號必須選擇至少一個推送事件",
  "账号规则选择的渠道均未启用": "帳號規則選擇的管道均未啟用",
  "推送设置已保存": "推送設定已儲存",
  "保存推送设置失败": "儲存推送設定失敗",
  "保存的渠道不存在": "未傳回已儲存的推送管道",
  "保存的规则不存在": "未傳回已儲存的推送規則",
  "测试推送成功": "測試推送成功",
  "测试推送失败": "測試推送失敗",
  "立即推送失败": "立即推送失敗",
  "加载推送日志失败": "載入推送紀錄失敗",
  "推送日志已清空": "推送紀錄已清空",
  "清空推送日志失败": "清空推送紀錄失敗",
  "显示 GPT 5.3 Codex Spark 额度": "顯示 GPT 5.3 Codex Spark 額度",
  "配额 / 余额": "配額 / 餘額",
  "余额": "餘額",
  "密钥额度": "金鑰額度",
  "套餐剩余": "方案剩餘",
  "刷新余额": "重新整理餘額",
  "点击刷新余额": "點擊重新整理餘額",
  "余额获取中": "正在讀取餘額",
  "余额获取失败": "讀取餘額失敗",
  "余额获取失败，点击重试": "讀取餘額失敗，點擊重試",
  "余额已刷新": "餘額已更新",
  "需确认": "需要確認",
  "HTTP 地址需点击确认后查询余额": "HTTP 位址需要確認，點擊後查詢餘額",
  "HTTP 连接安全提示": "HTTP 連線安全提示",
  "该中转站使用未加密的 HTTP 连接。查询余额时会通过此连接发送 API Key，同一网络中的设备或代理可能读取它。是否仅在本次运行期间允许？":
    "此中轉站使用未加密的 HTTP 連線。查詢餘額時會透過此連線傳送 API Key，同一網路中的裝置或代理可能讀取它。是否僅在本次執行期間允許？",
  "仅本次运行允许": "僅本次執行允許",
  "账号配置已变化，请重新点击余额并确认": "帳號設定已變更，請重新點擊餘額並確認",
  "获取失败": "讀取失敗",
  "更新于": "更新於",
  "不限量": "不限量",
  "从本机会话记录汇总 Tokens、缓存复用和预估费用": "從本機會話紀錄彙整 Tokens、快取重用和預估費用",
  "请选择有效的开始和结束时间，日期必须真实存在且开始时间不能晚于结束时间。":
    "請選擇有效的開始與結束時間，日期必須真實存在，且開始時間不能晚於結束時間。",
  "恢复内置 GPT/Codex 单价会覆盖当前 pricing.json，确定继续？":
    "還原內建 GPT/Codex 單價會覆寫目前的 pricing.json，確定要繼續嗎？",
  "维护 Codex 统计使用的模型单价和倍率": "維護 Codex 統計使用的模型單價和倍率",
  "设置统计倍率与模型识别来源": "設定統計倍率與模型識別來源",
  "有 {count} 个会话文件暂时无法读取，已跳过这些文件。":
    "有 {count} 個會話檔案暫時無法讀取，已略過這些檔案。",
  "新增输入 Tokens": "新增輸入 Tokens",
  "输出 Tokens": "輸出 Tokens",
  "缓存写入": "快取寫入",
  "缓存复用": "快取重用",
  "复用占比": "重用占比",
  "时段消耗曲线": "時段消耗曲線",
  "累计 Token 数": "累計 Token 數",
  "峰值 Token 数": "峰值 Token 數",
  "当前连续天数": "目前連續天數",
  "最长连续天数": "最長連續天數",
  "来源汇总": "來源彙整",
  "调用流水": "呼叫紀錄",
  "调用记录": "呼叫紀錄",
  "模型单价（每百万 Tokens）": "模型單價（每百萬 Tokens）",
  "Codex 计费口径": "Codex 計費口徑",
  "默认倍率": "預設倍率",
  "计费模式": "計費模式",
  "显示名称": "顯示名稱",
  "输入单价": "輸入單價",
  "输出单价": "輸出單價",
  "缓存复用 / 1M": "快取重用 / 1M",
  "缓存写入 / 1M": "快取寫入 / 1M",
  "恢复默认": "還原預設值",
  "暂无趋势数据": "暫無趨勢資料",
  "暂无活动数据": "暫無活動資料",
  "暂无使用记录": "暫無使用紀錄",
  "暂无来源数据": "暫無來源資料",
  "暂无模型用量": "暫無模型用量",
  "暂无模型单价": "暫無模型單價",
  "主要来源": "主要來源",
  "来源分布": "來源分布",
  "模型分布": "模型分布",
  "请求模型": "請求模型",
  "返回模型": "回傳模型",
  "计费模型": "計費模型",
  "平均延迟": "平均延遲",
  "费用倍率": "費用倍率",
  "费用规则": "費用規則",
  "预估费用": "預估費用",
  "总请求数": "總請求數",
  "请求数": "請求數",
  "统计": "統計",
  "费用": "費用",
  "模型单价": "模型單價",
  "内置": "內建",
  "单价": "單價",
  "口径": "口徑",
  "规则": "規則",
  "来源": "來源",
  "累计": "累計",
  "概览": "概覽",
  "无法": "無法",
  "跳过": "略過",
  "这些": "這些",
  "非负": "非負",
  "输入": "輸入",
  "输出": "輸出",
  "暂时": "暫時",
  "加载": "載入",
  "必须": "必須",
  "真实": "真實",
  "继续": "繼續",
  "确定": "確定",
  "开始": "開始",
  "结束": "結束",
  "日期": "日期",
  "活动": "活動",
  "趋势": "趨勢",
  "时段": "時段",
  "当": "當",
  "从": "從",
  "总": "總",
  "数": "數",
  "个": "個",
  "条": "條",
  与: "與",
  万: "萬",
  亿: "億",
  账号: "帳號",
  账号数: "帳號數",
  预约: "預約",
  日志: "日誌",
  该: "該",
  实际: "實際",
  服务端: "伺服器端",
  决定: "決定",
  历史: "歷史",
  额度: "額度",
  任务: "任務",
  执行: "執行",
  会话: "會話",
  管理: "管理",
  登录: "登入",
  态: "態",
  写: "寫",
  回: "回",
  本机: "本機",
  配置: "設定",
  总览: "總覽",
  使用统计: "使用統計",
  服务: "服務",
  当前: "目前",
  读取: "讀取",
  隐私: "隱私",
  样式: "樣式",
  侧: "側",
  边栏: "邊欄",
  添加: "新增",
  筛选: "篩選",
  邮箱: "信箱",
  昵称: "暱稱",
  创建: "建立",
  时间: "時間",
  周: "週",
  配额: "額度",
  重置: "重設",
  订阅: "訂閱",
  有效期: "有效期",
  顺序: "順序",
  编辑: "編輯",
  导出: "匯出",
  导入: "匯入",
  数据: "資料",
  应用: "應用程式",
  目录: "目錄",
  设置: "設定",
  备份: "備份",
  刷新: "重新整理",
  监控: "監控",
  自动: "自動",
  等待: "等待",
  倒计时: "倒數",
  保存: "儲存",
  外观: "外觀",
  固定: "固定",
  每页: "每頁",
  删除: "刪除",
  恢复: "還原",
  确认: "確認",
  取消: "取消",
  关闭: "關閉",
  打开: "開啟",
  文件: "檔案",
  状态: "狀態",
  密钥: "金鑰",
  运行: "執行",
  检测: "偵測",
  更新: "更新",
  版本: "版本",
  访问: "存取",
  地址: "位址",
  认证: "驗證",
  浏览器: "瀏覽器",
  授权: "授權",
  链接: "連結",
  复制: "複製",
  成功: "成功",
  失败: "失敗",
  错误: "錯誤",
  问题: "問題",
  反馈: "回饋",
  赞助: "贊助",
  支持: "支援",
  仓库: "倉庫",
  源码: "原始碼",
  记录: "紀錄",
  请输入: "請輸入",
  请选择: "請選擇",
};

export function normalizeLanguage(value: unknown): AppLanguage {
  if (value === "zh-TW" || value === "en" || value === "ru") return value;
  return "zh-CN";
}

export function setLanguage(value: unknown): void {
  const next = normalizeLanguage(value);
  currentLanguage.value = next;
  try {
    window.localStorage.setItem(storageKey, next);
  } catch {
    // localStorage 不可用时只在当前会话内生效。
  }
  queueTranslateDocument();
}

export function currentLocale(): string {
  if (currentLanguage.value === "zh-TW") return "zh-TW";
  if (currentLanguage.value === "en") return "en-US";
  if (currentLanguage.value === "ru") return "ru-RU";
  return "zh-CN";
}

export function formatLocalizedNumber(value?: number, options?: Intl.NumberFormatOptions): string {
  return new Intl.NumberFormat(currentLocale(), options).format(Number(value || 0));
}

export function formatCompactTokens(value?: number): string {
  const safe = Number(value || 0);
  if (currentLanguage.value === "zh-CN" || currentLanguage.value === "zh-TW") {
    if (safe >= 100_000_000) return `${(safe / 100_000_000).toFixed(1)} ${t("亿")}`;
    if (safe >= 10_000) return `${(safe / 10_000).toFixed(1)} ${t("万")}`;
    return formatLocalizedNumber(safe);
  }
  return formatLocalizedNumber(safe, {
    notation: "compact",
    maximumFractionDigits: safe >= 1_000 ? 1 : 0,
  });
}

export function formatLocalizedCount(count: number, unit: string): string {
  const number = formatLocalizedNumber(count);
  if (currentLanguage.value === "en") {
    const units: Record<string, string> = {
      天: "days",
      次: "times",
      个: "items",
      条记录: "records",
      条规则: "rules",
      条会话: "sessions",
      个账号: "accounts",
    };
    return `${number} ${units[unit] || t(unit)}`;
  }
  if (currentLanguage.value === "ru") {
    const units: Record<string, string> = {
      天: "дн.",
      次: "раз",
      个: "шт.",
      条记录: "записей",
      条规则: "правил",
      条会话: "сессий",
      个账号: "аккаунтов",
    };
    return `${number} ${units[unit] || t(unit)}`;
  }
  return `${number} ${t(unit)}`;
}

export function formatLocalizedDuration(days: number, hours: number, minutes = 0): string {
  if (currentLanguage.value === "en") {
    return days > 0 ? `${days}d${hours}h` : `${hours}h${minutes}m`;
  }
  if (currentLanguage.value === "ru") {
    return days > 0 ? `${days}д${hours}ч` : `${hours}ч${minutes}м`;
  }
  return days > 0 ? `${days}${t("天")}${hours}${t("小时")}` : `${hours}${t("小时")}${minutes}${t("分钟")}`;
}

export function t(value: unknown): string {
  if (typeof value !== "string" || !value.trim()) return String(value ?? "");
  const lang = currentLanguage.value;
  const leading = value.match(/^\s*/)?.[0] ?? "";
  const trailing = value.match(/\s*$/)?.[0] ?? "";
  const body = value.trim();
  if (!body) return value;
  const sourceBody = restoreSourceText(body);
  if (lang === "zh-CN") return `${leading}${sourceBody}${trailing}`;
  if (lang === "zh-TW") return `${leading}${toTraditional(sourceBody)}${trailing}`;
  const translated = translateWithDictionary(sourceBody, dictionary[lang]);
  return `${leading}${translated}${trailing}`;
}

export function formatTranslatedText(
  template: string,
  values: Record<string, string | number>,
): string {
  return t(template).replace(/\{([A-Za-z0-9_]+)\}/g, (placeholder, key: string) =>
    Object.prototype.hasOwnProperty.call(values, key) ? String(values[key]) : placeholder,
  );
}

export function installDomI18n(root: HTMLElement = document.body): void {
  translateElement(root);
  observer?.disconnect();
  stopLanguageWatch?.();
  observer = new MutationObserver(() => queueTranslateDocument());
  observer.observe(root, {
    childList: true,
    subtree: true,
    characterData: true,
    attributes: true,
    attributeFilter: ["placeholder", "title", "aria-label"],
  });
  stopLanguageWatch = watch(currentLanguage, () => queueTranslateDocument());
}

export function installArcoI18n(message: ArcoMessage, modal: ArcoModal): void {
  if (!patchedMessages.has(message)) {
    patchedMessages.add(message);
    for (const method of ["info", "success", "warning", "error", "normal", "loading"] as MessageMethod[]) {
      const original = message[method]?.bind(message);
      if (!original) continue;
      (message[method] as unknown) = (config: unknown) =>
        (original as AnyInvoker)(translateConfig(config));
    }
  }
  if (!patchedModals.has(modal)) {
    patchedModals.add(modal);
    for (const method of ["info", "success", "warning", "error", "confirm"] as ModalMethod[]) {
      const original = modal[method]?.bind(modal);
      if (!original) continue;
      (modal[method] as unknown) = (config: unknown) =>
        (original as AnyInvoker)(translateConfig(config));
    }
  }
}

function readStoredLanguage(): AppLanguage {
  try {
    return normalizeLanguage(window.localStorage.getItem(storageKey));
  } catch {
    return "zh-CN";
  }
}

function queueTranslateDocument(): void {
  if (pendingTranslate) return;
  pendingTranslate = true;
  void nextTick(() => {
    window.requestAnimationFrame(() => {
      pendingTranslate = false;
      translateElement(document.body);
    });
  });
}

function translateConfig(config: unknown): unknown {
  if (typeof config === "string") return t(config);
  if (!config || typeof config !== "object" || Array.isArray(config)) return config;
  const next = { ...(config as Record<string, unknown>) };
  for (const key of ["content", "title", "okText", "cancelText", "message"]) {
    if (typeof next[key] === "string") next[key] = t(next[key]);
  }
  return next;
}

function translateElement(root: HTMLElement): void {
  if (shouldSkipElement(root)) return;
  translateAttributes(root);
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT | NodeFilter.SHOW_TEXT);
  let node = walker.nextNode();
  while (node) {
    if (node.nodeType === Node.ELEMENT_NODE) {
      const element = node as HTMLElement;
      if (shouldSkipElement(element)) {
        node = walker.nextSibling();
        continue;
      }
      translateAttributes(element);
    } else if (node.nodeType === Node.TEXT_NODE) {
      translateTextNode(node as Text);
    }
    node = walker.nextNode();
  }
}

function translateTextNode(node: Text): void {
  const cached = textNodeSources.get(node);
  const current = node.nodeValue ?? "";
  const source = cached && isRenderedTranslation(cached, current) ? cached : current;
  if (!source.trim()) return;
  textNodeSources.set(node, source);
  const next = t(source);
  if (node.nodeValue !== next) node.nodeValue = next;
}

function translateAttributes(element: HTMLElement): void {
  for (const attr of ["placeholder", "title", "aria-label"]) {
    const value = element.getAttribute(attr);
    if (!value?.trim()) continue;
    const sourceAttr = `data-i18n-source-${attr}`;
    const source = element.getAttribute(sourceAttr) || value;
    if (!element.hasAttribute(sourceAttr)) element.setAttribute(sourceAttr, source);
    const next = t(source);
    if (value !== next) element.setAttribute(attr, next);
  }
}

function shouldSkipElement(element: HTMLElement): boolean {
  const tag = element.tagName.toLowerCase();
  return ["script", "style", "code", "pre", "textarea"].includes(tag);
}

function translateWithDictionary(text: string, dict: Record<string, string>): string {
  if (dict[text]) return dict[text];
  return translateDynamicText(text, dict) ?? text;
}

function restoreSourceText(text: string): string {
  const reverse = getReverseDictionary();
  return reverse.get(text) || restoreDynamicSourceText(text) || text;
}

function getReverseDictionary(): Map<string, string> {
  if (reverseDictionary) return reverseDictionary;
  const next = new Map<string, string>();
  for (const source of Object.keys(en)) {
    next.set(en[source], source);
  }
  for (const source of Object.keys(ru)) {
    next.set(ru[source], source);
  }
  reverseDictionary = next;
  return next;
}

function restoreDynamicSourceText(text: string): string | null {
  const pageText = text.match(/^(?:Page|Страница)\s+(\d+)\s*\/\s*(\d+)$/);
  if (pageText) return `第 ${pageText[1]} / ${pageText[2]} 页`;

  const ordinal = text.match(/^(?:#|№)\s*(\d+)$/);
  if (ordinal) return `第 ${ordinal[1]} 次`;

  const updated = text.match(/^(?:Updated|Обновлено)\s+(.+)$/);
  if (updated) return `更新 ${updated[1]}`;

  const records = text.match(/^(\d+)\s+(?:records|записей)$/);
  if (records) return `共 ${records[1]} 条记录`;

  const countUnit = text.match(/^(\d+)\s+(days|times|items|records|rules|sessions|accounts|дн\.|раз|шт\.|записей|правил|сессий|аккаунтов)$/);
  if (countUnit) {
    const units: Record<string, string> = {
      days: "天",
      "дн.": "天",
      times: "次",
      "раз": "次",
      items: "个",
      "шт.": "个",
      records: "条记录",
      "записей": "条记录",
      rules: "条规则",
      "правил": "条规则",
      sessions: "条会话",
      "сессий": "条会话",
      accounts: "个账号",
      "аккаунтов": "个账号",
    };
    const unit = units[countUnit[2]];
    return unit ? `${countUnit[1]} ${unit}` : null;
  }

  const windowText = text.match(/^(?:(\d+)\s+min window|Окно\s+(\d+)\s+мин)$/);
  if (windowText) return `${windowText[1] || windowText[2]} 分钟窗口`;

  const hourWindowText = text.match(/^(?:(\d+)h window|Окно\s+(\d+)\s+ч)$/);
  if (hourWindowText) return `${hourWindowText[1] || hourWindowText[2]} 小时窗口`;

  const dayWindowText = text.match(/^(?:(\d+)\s+day window|Окно\s+(\d+)\s+дн\.)$/);
  if (dayWindowText) return `${dayWindowText[1] || dayWindowText[2]} 天窗口`;

  const averageCost = text.match(/^(.+)\s+(?:times|раз)\s*·\s*(?:avg|сред\.)\s*(.+)$/);
  if (averageCost) return `${averageCost[1]} 次 · 平均 ${averageCost[2]}`;

  const costText = text.match(/^(.+)\s+(?:times|раз)\s*·\s*(.+)$/);
  if (costText) return `${costText[1]} 次 · ${costText[2]}`;

  return null;
}

function isRenderedTranslation(source: string, current: string): boolean {
  const restored = restoreSourceText(current);
  return (
    current === source ||
    restored === source ||
    current === toTraditional(source) ||
    current === translateWithDictionary(source, en) ||
    current === translateWithDictionary(source, ru)
  );
}

function translateDynamicText(text: string, dict: Record<string, string>): string | null {
  const isRussian = dict === ru;
  const isEnglish = dict === en;
  if (!isRussian && !isEnglish) return null;

  const countUnit = text.match(/^(\d+)\s*(天|次|个|条记录|条规则|条会话|个账号)$/);
  if (countUnit) {
    const [, count, unit] = countUnit;
    const units: Record<string, [string, string]> = {
      天: ["days", "дн."],
      次: ["times", "раз"],
      个: ["items", "шт."],
      条记录: ["records", "записей"],
      条规则: ["rules", "правил"],
      条会话: ["sessions", "сессий"],
      个账号: ["accounts", "аккаунтов"],
    };
    const translatedUnit = units[unit]?.[isEnglish ? 0 : 1];
    return translatedUnit ? `${count} ${translatedUnit}` : null;
  }

  const totalRecords = text.match(/^共\s*(\d+)\s*条记录$/);
  if (totalRecords) {
    return isEnglish ? `${totalRecords[1]} records` : `${totalRecords[1]} записей`;
  }

  const pageText = text.match(/^第\s*(\d+)\s*\/\s*(\d+)\s*页$/);
  if (pageText) {
    return isEnglish
      ? `Page ${pageText[1]} / ${pageText[2]}`
      : `Страница ${pageText[1]} / ${pageText[2]}`;
  }

  const ordinalUse = text.match(/^第\s*(\d+)\s*次$/);
  if (ordinalUse) {
    return isEnglish ? `#${ordinalUse[1]}` : `№ ${ordinalUse[1]}`;
  }

  const manualBackupProgress = text.match(/^手动备份\s*(\d+)%$/);
  if (manualBackupProgress) {
    return isEnglish
      ? `Manual Backup ${manualBackupProgress[1]}%`
      : `Создание копии ${manualBackupProgress[1]}%`;
  }

  const backupProgress = text.match(/^备份\s*(\d+)%$/);
  if (backupProgress) {
    return isEnglish
      ? `Backup ${backupProgress[1]}%`
      : `Копия ${backupProgress[1]}%`;
  }

  const minutesWindow = text.match(/^(\d+)\s*分钟窗口$/);
  if (minutesWindow) {
    return isEnglish
      ? `${minutesWindow[1]} min window`
      : `Окно ${minutesWindow[1]} мин`;
  }

  const hoursWindow = text.match(/^(\d+)\s*小时窗口$/);
  if (hoursWindow) {
    return isEnglish
      ? `${hoursWindow[1]}h window`
      : `Окно ${hoursWindow[1]} ч`;
  }

  const daysWindow = text.match(/^(\d+)\s*天窗口$/);
  if (daysWindow) {
    return isEnglish
      ? `${daysWindow[1]} day window`
      : `Окно ${daysWindow[1]} дн.`;
  }

  const updatedAt = text.match(/^更新\s+(.+)$/);
  if (updatedAt) {
    return isEnglish ? `Updated ${updatedAt[1]}` : `Обновлено ${updatedAt[1]}`;
  }

  const averageCost = text.match(/^(.+)\s*次\s*·\s*平均\s*(.+)$/);
  if (averageCost) {
    return isEnglish
      ? `${averageCost[1]} times · avg ${averageCost[2]}`
      : `${averageCost[1]} раз · сред. ${averageCost[2]}`;
  }

  const costText = text.match(/^(.+)\s*次\s*·\s*(.+)$/);
  if (costText) {
    return isEnglish
      ? `${costText[1]} times · ${costText[2]}`
      : `${costText[1]} раз · ${costText[2]}`;
  }

  const deleteAccount = text.match(/^确认删除\s+(.+)？此操作只删除本工具保存的账号记录，不会删除 Codex 程序本身。$/);
  if (deleteAccount) {
    return isEnglish
      ? `Delete ${deleteAccount[1]}? This only removes the account record saved by this tool. The Codex app itself will not be deleted.`
      : `Удалить ${deleteAccount[1]}? Будет удалена только запись аккаунта в этом инструменте. Само приложение Codex не удаляется.`;
  }

  const resetConfig = text.match(/^确认删除本机 Codex 目录下的 config\.toml？删除后 Codex 会按默认配置重新生成或使用默认设置。$/);
  if (resetConfig) {
    return isEnglish
      ? "Delete config.toml under the local Codex directory? Codex will recreate it or use defaults afterward."
      : "Удалить config.toml из локальной папки Codex? После этого Codex создаст его заново или использует настройки по умолчанию.";
  }

  const resetApiService = text.match(/^将先停止 API 服务，然后删除 ~\/\.codex_switcher\/api-service 下的运行时、配置、工作区和下载缓存。此操作不会删除账号总览里的账号，是否继续？$/);
  if (resetApiService) {
    return isEnglish
      ? "The API service will be stopped, then runtime, config, workspace, and download cache under ~/.codex_switcher/api-service will be removed. Accounts in the overview will not be deleted. Continue?"
      : "API-сервис будет остановлен, затем будут удалены runtime, конфигурация, рабочая папка и кэш загрузок в ~/.codex_switcher/api-service. Аккаунты в обзоре не удаляются. Продолжить?";
  }

  const trashSessions = text.match(/^确认将\s+(\d+)\s+个会话移入回收站？移入后可以在回收站中恢复。$/);
  if (trashSessions) {
    return isEnglish
      ? `Move ${trashSessions[1]} sessions to Trash? They can be restored from Trash later.`
      : `Переместить ${trashSessions[1]} сессий в корзину? Их можно будет восстановить из корзины.`;
  }

  const movedSessions = text.match(/^已移动\s+(\d+)\s+个会话到回收站$/);
  if (movedSessions) {
    return isEnglish
      ? `Moved ${movedSessions[1]} sessions to Trash`
      : `${movedSessions[1]} сессий перемещено в корзину`;
  }

  const restoredSessions = text.match(/^已恢复\s+(\d+)\s+个会话$/);
  if (restoredSessions) {
    return isEnglish
      ? `Restored ${restoredSessions[1]} sessions`
      : `Восстановлено ${restoredSessions[1]} сессий`;
  }

  const unreadableSessions = text.match(/^有\s*(\d+)\s*个会话文件暂时无法读取，已跳过这些文件。$/);
  if (unreadableSessions) {
    return isEnglish
      ? `${unreadableSessions[1]} session files could not be read and were skipped.`
      : `${unreadableSessions[1]} файлов сессий не удалось прочитать, они пропущены.`;
  }

  const updateTitle = text.match(/^发现新版本\s+(.+)$/);
  if (updateTitle) {
    return isEnglish
      ? `New version available ${updateTitle[1]}`
      : `Доступна новая версия ${updateTitle[1]}`;
  }

  const updateAvailable = text.match(
    /^当前版本\s+(.+)，最新版本\s+(.+)。请前往 GitHub Releases 下载最新安装包。$/,
  );
  if (updateAvailable) {
    return isEnglish
      ? `Current version ${updateAvailable[1]}, latest version ${updateAvailable[2]}. Open GitHub Releases to download the latest installer.`
      : `Текущая версия ${updateAvailable[1]}, последняя версия ${updateAvailable[2]}. Откройте GitHub Releases, чтобы скачать новый установщик.`;
  }

  const updateCurrent = text.match(
    /^当前版本\s+(.+)，最新发布版本\s+(.+)。如需重新下载安装包，可以打开 GitHub Releases。$/,
  );
  if (updateCurrent) {
    return isEnglish
      ? `Current version ${updateCurrent[1]}, latest release ${updateCurrent[2]}. You can open GitHub Releases to download the installer again.`
      : `Текущая версия ${updateCurrent[1]}, последний релиз ${updateCurrent[2]}. Можно открыть GitHub Releases, чтобы скачать установщик заново.`;
  }

  const updateFailed = text.match(/^暂时无法获取最新版本信息：(.+)。可以前往 GitHub Releases 手动查看。$/);
  if (updateFailed) {
    return isEnglish
      ? `Could not fetch the latest version right now: ${updateFailed[1]}. You can open GitHub Releases manually.`
      : `Не удалось получить последнюю версию: ${updateFailed[1]}. Можно открыть GitHub Releases вручную.`;
  }

  return null;
}

function toTraditional(text: string): string {
  let next = text;
  for (const key of Object.keys(simplifiedToTraditional).sort((left, right) => right.length - left.length)) {
    next = next.split(key).join(simplifiedToTraditional[key]);
  }
  return next;
}
