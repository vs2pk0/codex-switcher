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
  "全选": "Select All",
  "筛选邮箱 / 昵称": "Filter email / nickname",
  "无数据": "No Data",
  "按创建时间": "By creation time",
  "按周配额": "By weekly quota",
  "按5小时配额": "By 5-hour quota",
  "按周配额重置时间": "By weekly reset time",
  "按5小时配额重置时间": "By 5-hour reset time",
  "按订阅有效期": "By subscription expiry",
  "自定义顺序": "Custom order",
  "编辑排序": "Edit order",
  "倒序": "Desc",
  "正序": "Asc",
  "每页": "Per page",
  "绑定到 API 服务": "Bind to API Service",
  "批量导出": "Batch Export",
  "批量导入": "Batch Import",
  "当前账号": "Current Account",
  "当前页": "Current Page",
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
  "刷新": "Refresh",
  "监控额度": "Monitor Quota",
  "额度自动刷新": "Quota Auto Refresh",
  "当前账号刷新": "Current Account Refresh",
  "等待刷新倒计时": "Refresh Countdown",
  "保存": "Save",
  "外观": "Appearance",
  "每行固定账号数": "Fixed Accounts Per Row",
  "每页账号数": "Accounts Per Page",
  "配置": "Config",
  "重置 config.toml": "Reset config.toml",
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
  "本地 Codex 消耗": "Local Codex Usage",
  "总请求数": "Requests",
  "预估费用": "Estimated Cost",
  "输入 Tokens": "Input Tokens",
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
  "请选择要删除的 API 服务账号": "Select API service accounts to delete",
  "认证目录里暂无账号": "No accounts in the auth directory",
  "API 服务已更新并重新启动": "API service updated and restarted",
  "API 服务更新已安装": "API service update installed",
  "下载已取消": "Download cancelled",
  "服务运行中不能修改端口或密钥，请先停止服务。":
    "Ports and keys cannot be changed while the service is running. Stop the service first.",
  "绑定账号到 API 服务": "Bind Accounts to API Service",
  "选择 OAuth 账号后会转换为 CPA 格式，并写入 API 服务的认证目录。":
    "Selected OAuth accounts are converted to CPA format and written to the API service auth directory.",
  "已选": "Selected",
  "确认绑定": "Confirm Bind",
  "删除 API 服务账号": "Delete API Service Accounts",
  "这里从认证目录 JSON 内容解析邮箱匹配账号，删除会移除对应 CPA 认证文件。":
    "Accounts are matched by email parsed from auth-directory JSON. Deleting removes the matching CPA auth file.",
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
  "选择重置次数": "Choose Reset Credit",
  "重置次数": "Reset Credit",
  "选择要消耗的重置次数": "Choose a reset credit to use",
  "当前有": "currently has",
  "次可用": "available",
  "发放": "Granted",
  "暂无重置次数明细，请先刷新额度": "No reset credit details. Refresh quota first.",
  "重置使用次数": "Use Reset Credit",
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
  "查看最新版本并下载安装包": "View the latest version and download installers",
  "当前已是最新版本": "You are up to date",
  "检查更新失败": "Update check failed",
  "前往下载": "Go to Download",
  "稍后再说": "Later",
  "打开 Releases": "Open Releases",
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
  "全选": "Выбрать все",
  "筛选邮箱 / 昵称": "Фильтр email / имя",
  "无数据": "Нет данных",
  "按创建时间": "По времени создания",
  "按周配额": "По недельной квоте",
  "按5小时配额": "По квоте 5 часов",
  "按周配额重置时间": "По сбросу недельной квоты",
  "按5小时配额重置时间": "По сбросу квоты 5 часов",
  "按订阅有效期": "По сроку подписки",
  "自定义顺序": "Свой порядок",
  "编辑排序": "Изменить порядок",
  "倒序": "Убыв.",
  "正序": "Возр.",
  "每页": "На странице",
  "绑定到 API 服务": "Привязать к API",
  "批量导出": "Экспорт",
  "批量导入": "Импорт",
  "当前账号": "Текущий аккаунт",
  "当前页": "Текущая страница",
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
  "刷新": "Обновление",
  "监控额度": "Мониторинг квоты",
  "额度自动刷新": "Автообновление квоты",
  "当前账号刷新": "Обновление текущего аккаунта",
  "等待刷新倒计时": "Таймер обновления",
  "保存": "Сохранить",
  "外观": "Внешний вид",
  "每行固定账号数": "Аккаунтов в строке",
  "每页账号数": "Аккаунтов на странице",
  "配置": "Конфигурация",
  "重置 config.toml": "Сбросить config.toml",
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
  "本地 Codex 消耗": "Локальный расход Codex",
  "总请求数": "Запросы",
  "预估费用": "Оценка стоимости",
  "输入 Tokens": "Входные токены",
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
  "请选择要删除的 API 服务账号": "Выберите аккаунты API-сервиса для удаления",
  "认证目录里暂无账号": "В каталоге авторизации пока нет аккаунтов",
  "API 服务已更新并重新启动": "API-сервис обновлен и перезапущен",
  "API 服务更新已安装": "Обновление API-сервиса установлено",
  "下载已取消": "Загрузка отменена",
  "服务运行中不能修改端口或密钥，请先停止服务。":
    "Порт и ключи нельзя менять во время работы сервиса. Сначала остановите сервис.",
  "绑定账号到 API 服务": "Привязать аккаунты к API-сервису",
  "选择 OAuth 账号后会转换为 CPA 格式，并写入 API 服务的认证目录。":
    "Выбранные OAuth-аккаунты будут преобразованы в формат CPA и записаны в каталог авторизации API-сервиса.",
  "已选": "Выбрано",
  "确认绑定": "Подтвердить привязку",
  "删除 API 服务账号": "Удалить аккаунты API-сервиса",
  "这里从认证目录 JSON 内容解析邮箱匹配账号，删除会移除对应 CPA 认证文件。":
    "Аккаунты сопоставляются по email из JSON в каталоге авторизации. Удаление уберет соответствующий CPA-файл.",
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
  "选择重置次数": "Выбрать сброс",
  "重置次数": "Сбросы",
  "选择要消耗的重置次数": "Выберите сброс для использования",
  "当前有": "сейчас доступно",
  "次可用": "доступно",
  "发放": "Выдано",
  "暂无重置次数明细，请先刷新额度": "Нет сведений о сбросах. Сначала обновите квоту.",
  "重置使用次数": "Использовать сброс",
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
  "查看最新版本并下载安装包": "Посмотреть последнюю версию и скачать установщик",
  "当前已是最新版本": "У вас последняя версия",
  "检查更新失败": "Не удалось проверить обновления",
  "前往下载": "Перейти к загрузке",
  "稍后再说": "Позже",
  "打开 Releases": "Открыть Releases",
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
  与: "與",
  万: "萬",
  亿: "億",
  账号: "帳號",
  账号数: "帳號數",
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
