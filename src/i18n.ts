import { nextTick, ref } from "vue";
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
  "账号总览": "Accounts",
  "会话管理": "Sessions",
  "使用统计": "Usage",
  "API 服务": "API Service",
  "设置": "Settings",
  "关于": "About",
  "全部": "All",
  "当前": "Current",
  "读取当前账号": "Detect Current",
  "已隐藏": "Hidden",
  "隐私": "Privacy",
  "徽章样式": "Badge Style",
  "添加账号": "Add Account",
  "全选": "Select All",
  "筛选邮箱 / 昵称": "Filter email / nickname",
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
  "还没有备份": "No backups yet",
  "手动备份": "Manual Backup",
  "取消": "Cancel",
  "确认": "Confirm",
  "关闭": "Close",
  "导入": "Import",
  "添加": "Add",
  "编辑": "Edit",
  "切换": "Switch",
  "置顶账号": "Pin account",
  "取消置顶": "Unpin account",
  "绑定手机": "Bind Phone",
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
  "复用占比": "Reuse Rate",
  "趋势": "Trend",
  "请求明细": "Request Logs",
  "Provider 统计": "Provider Stats",
  "模型统计": "Model Stats",
  "费用规则": "Pricing",
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
  "服务配置": "Service Config",
  "端口": "Port",
  "管理密钥": "Admin Key",
  "自动更新": "Auto Update",
  "检测间隔": "Check Interval",
  "API 密钥": "API Keys",
  "随机重生成": "Regenerate",
  "添加密钥": "Add Key",
  "保存配置": "Save Config",
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
  "生成并打开授权页": "Generate and Open Auth Page",
  "继续打开授权页": "Open Auth Page Again",
  "复制链接": "Copy Link",
  "手动完成": "Manual Completion",
  "完成接入": "Complete",
  "重试保存": "Retry Save",
  "从本地文件导入": "Import from local file",
  "获取本地账号": "Import local account",
  "账号名称": "Account Name",
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
  "选择重置次数": "Choose Reset Credit",
  "重置使用次数": "Use Reset Credit",
  "找回会话显示": "Recover Session Visibility",
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
  "当前实例": "Current Instance",
  "本机全部": "All Local",
};

const ru: Record<string, string> = {
  "中文": "Китайский",
  "繁體中文（台灣）": "Традиционный китайский (Тайвань)",
  "English": "Английский",
  "Русский": "Русский",
  "Codex Switcher": "Codex Switcher",
  "账号总览": "Аккаунты",
  "会话管理": "Сессии",
  "使用统计": "Статистика",
  "API 服务": "API-сервис",
  "设置": "Настройки",
  "关于": "О программе",
  "全部": "Все",
  "当前": "Текущий",
  "读取当前账号": "Определить текущий",
  "已隐藏": "Скрыто",
  "隐私": "Приватность",
  "徽章样式": "Стиль бейджа",
  "添加账号": "Добавить аккаунт",
  "全选": "Выбрать все",
  "筛选邮箱 / 昵称": "Фильтр email / имя",
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
  "还没有备份": "Резервных копий пока нет",
  "手动备份": "Создать копию",
  "取消": "Отмена",
  "确认": "Подтвердить",
  "关闭": "Закрыть",
  "导入": "Импорт",
  "添加": "Добавить",
  "编辑": "Изменить",
  "切换": "Переключить",
  "置顶账号": "Закрепить",
  "取消置顶": "Открепить",
  "绑定手机": "Привязать телефон",
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
  "消耗看板": "Панель расхода",
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
  "复用占比": "Доля повтора",
  "趋势": "Тренд",
  "请求明细": "Журнал запросов",
  "Provider 统计": "Статистика провайдеров",
  "模型统计": "Статистика моделей",
  "费用规则": "Цены",
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
  "服务配置": "Конфигурация сервиса",
  "端口": "Порт",
  "管理密钥": "Ключ администратора",
  "自动更新": "Автообновление",
  "检测间隔": "Интервал проверки",
  "API 密钥": "API-ключи",
  "随机重生成": "Сгенерировать",
  "添加密钥": "Добавить ключ",
  "保存配置": "Сохранить",
  "更新信息": "Обновления",
  "最新版本": "Последняя версия",
  "匹配平台": "Платформа",
  "上次检测": "Последняя проверка",
  "本地目录": "Локальные папки",
  "浏览器登录，自动带回授权结果": "Войдите в браузере, результат вернется автоматически.",
  "生成并打开授权页": "Создать и открыть страницу",
  "继续打开授权页": "Открыть страницу снова",
  "复制链接": "Копировать ссылку",
  "手动完成": "Завершить вручную",
  "完成接入": "Готово",
  "重试保存": "Повторить сохранение",
  "导出 JSON": "Экспорт JSON",
  "批量导出 JSON": "Массовый экспорт JSON",
  "导出格式": "Формат экспорта",
  "预览": "Предпросмотр",
  "隐藏预览": "Скрыть предпросмотр",
  "复制": "Копировать",
  "下载": "Скачать",
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
};

const dictionary: Record<Exclude<AppLanguage, "zh-CN" | "zh-TW">, Record<string, string>> = {
  en,
  ru,
};

const simplifiedToTraditional: Record<string, string> = {
  与: "與",
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

export function t(value: unknown): string {
  if (typeof value !== "string" || !value.trim()) return String(value ?? "");
  const lang = currentLanguage.value;
  if (lang === "zh-CN") return value;
  const leading = value.match(/^\s*/)?.[0] ?? "";
  const trailing = value.match(/\s*$/)?.[0] ?? "";
  const body = value.trim();
  if (!body) return value;
  if (lang === "zh-TW") return `${leading}${toTraditional(body)}${trailing}`;
  const translated = translateWithDictionary(body, dictionary[lang]);
  return `${leading}${translated}${trailing}`;
}

export function installDomI18n(root: HTMLElement = document.body): void {
  translateElement(root);
  observer?.disconnect();
  observer = new MutationObserver(() => queueTranslateDocument());
  observer.observe(root, {
    childList: true,
    subtree: true,
    characterData: true,
    attributes: true,
    attributeFilter: ["placeholder", "title", "aria-label"],
  });
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
  let next = text;
  for (const key of Object.keys(dict).sort((left, right) => right.length - left.length)) {
    if (!next.includes(key)) continue;
    next = next.split(key).join(dict[key]);
  }
  return next;
}

function isRenderedTranslation(source: string, current: string): boolean {
  return (
    current === source ||
    current === toTraditional(source) ||
    current === translateWithDictionary(source, en) ||
    current === translateWithDictionary(source, ru)
  );
}

function toTraditional(text: string): string {
  let next = text;
  for (const key of Object.keys(simplifiedToTraditional).sort((left, right) => right.length - left.length)) {
    next = next.split(key).join(simplifiedToTraditional[key]);
  }
  return next;
}
