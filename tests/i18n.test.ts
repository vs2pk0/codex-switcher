import test, { afterEach } from "node:test";
import assert from "node:assert/strict";
import { currentLanguage, formatTranslatedText, t } from "../src/i18n.ts";

const originalLanguage = currentLanguage.value;

afterEach(() => {
  currentLanguage.value = originalLanguage;
});

test("俄语重置页面核心文案不会回退为中文", () => {
  currentLanguage.value = "ru";

  assert.equal(t("重置记录"), "История сбросов");
  assert.equal(t("取消预约"), "Отменить сброс");
  assert.equal(t("重置日志"), "Журналы сброса");
});

test("繁体中文重置页面不会简繁混排", () => {
  currentLanguage.value = "zh-TW";

  assert.equal(t("预约重置"), "預約重設");
  assert.equal(t("确认取消该预约？"), "確認取消該預約？");
  assert.equal(t("重置日志"), "重設日誌");
});

test("带账号和错误详情的重置提示会翻译静态文案", () => {
  const template = "{account} 预约重置失败：{error}";
  const values = {
    account: "demo@example.com",
    error: "network unavailable",
  };

  currentLanguage.value = "en";
  assert.equal(
    formatTranslatedText(template, values),
    "Scheduled reset failed for demo@example.com: network unavailable",
  );

  currentLanguage.value = "ru";
  assert.equal(
    formatTranslatedText(template, values),
    "Не удалось выполнить запланированный сброс для demo@example.com: network unavailable",
  );

  currentLanguage.value = "zh-TW";
  assert.equal(
    formatTranslatedText(template, values),
    "demo@example.com 預約重設失敗：network unavailable",
  );
});

test("英文设置、多开与 OpenCodex 页面不会回退为中文", () => {
  currentLanguage.value = "en";

  assert.equal(t("当前设置实例"), "Current Settings Instance");
  assert.equal(t("当前会话实例"), "Current Session Instance");
  assert.equal(
    t("官方实例会包含在默认完整备份中，也可以在这里单独手动备份。"),
    "The official instance is included in the default full backup. You can also back it up separately here.",
  );
  assert.equal(
    t("多开实例不会进入默认完整备份，只能在这里手动备份。"),
    "Additional instances are excluded from the default full backup and must be backed up manually here.",
  );
  assert.equal(t("Codex 多开"), "Codex Instances");
  assert.equal(t("运行控制台"), "Control Console");
  assert.equal(t("Web 管理"), "Web Management");
  assert.equal(t("启动服务后配置图片模型"), "Start the Service to Configure Vision Models");
  assert.equal(t("后台服务尚未安装"), "The background service is not installed");
  assert.equal(t("源文件中不存在此账号"), "This account does not exist in the source file");
  assert.equal(
    t("请先在“运行控制台”启动服务，再打开 Web 管理页面。"),
    "Start the service in Control Console before opening Web Management.",
  );
});

test("俄语设置、多开与 OpenCodex 页面不会回退为中文", () => {
  currentLanguage.value = "ru";

  assert.equal(t("当前设置实例"), "Текущий экземпляр настроек");
  assert.equal(t("当前会话实例"), "Текущий экземпляр сессий");
  assert.equal(
    t("多开实例不会进入默认完整备份，只能在这里手动备份。"),
    "Дополнительные экземпляры не входят в полную резервную копию по умолчанию; их нужно сохранять здесь вручную.",
  );
  assert.equal(t("Codex 多开"), "Экземпляры Codex");
  assert.equal(t("运行控制台"), "Панель управления");
  assert.equal(t("图片模型"), "Модели изображений");
  assert.equal(t("后台服务尚未安装"), "Фоновая служба не установлена");
  assert.equal(t("源文件中不存在此账号"), "Этого аккаунта нет в исходном файле");
  assert.equal(
    t("启动服务后配置图片模型"),
    "Запустите сервис для настройки моделей изображений",
  );
});

test("繁体中文设置、多开与 OpenCodex 页面不会简繁混排", () => {
  currentLanguage.value = "zh-TW";

  assert.equal(t("当前设置实例"), "目前設定執行個體");
  assert.equal(t("启动服务后配置图片模型"), "啟動服務後設定圖片模型");
  assert.equal(
    t("为每个桌面实例隔离账号、配置、会话、插件与 Electron 数据。"),
    "為每個桌面執行個體隔離帳號、設定、會話、外掛與 Electron 資料。",
  );
});

test("账号绑定入口与 OpenCodex 绑定提示支持全部界面语言", () => {
  currentLanguage.value = "en";
  assert.equal(t("绑定"), "Bind");
  assert.equal(t("绑定到 OpenCodex"), "Bind to OpenCodex");
  assert.equal(
    formatTranslatedText("已绑定 {count} 个账号到 OpenCodex", { count: 2 }),
    "Bound 2 accounts to OpenCodex",
  );

  currentLanguage.value = "ru";
  assert.equal(t("绑定"), "Привязать");
  assert.equal(t("绑定到 OpenCodex"), "Привязать к OpenCodex");

  currentLanguage.value = "zh-TW";
  assert.equal(t("绑定"), "綁定");
  assert.equal(t("请先启动 OpenCodex 服务，再绑定账号"), "請先啟動 OpenCodex 服務，再綁定帳號");
});

test("会话统计安全提示不会误报为文件读取失败", () => {
  const message = "有 1 个会话统计项需要注意，已保留可确认的可信数据。";

  currentLanguage.value = "en";
  assert.equal(
    t(message),
    "Session usage warnings: 1. Verified data has been preserved.",
  );

  currentLanguage.value = "ru";
  assert.equal(
    t(message),
    "Предупреждения статистики сеансов: 1. Подтвержденные данные сохранены.",
  );

  currentLanguage.value = "zh-TW";
  assert.equal(t(message), "有 1 個會話統計項需要注意，已保留可確認的可信資料。");
});

test("统计实例范围支持全部界面语言", () => {
  currentLanguage.value = "en";
  assert.equal(t("统计范围"), "Statistics Scope");
  assert.equal(t("全部实例"), "All Instances");

  currentLanguage.value = "ru";
  assert.equal(t("统计范围"), "Область статистики");
  assert.equal(t("全部实例"), "Все экземпляры");

  currentLanguage.value = "zh-TW";
  assert.equal(t("统计范围"), "統計範圍");
  assert.equal(t("全部实例"), "全部執行個體");
});

test("账号 JSON 格式标签支持全部界面语言", () => {
  currentLanguage.value = "en";
  assert.equal(t("Switcher JSON"), "Switcher JSON");
  assert.equal(t("Token JSON"), "Token JSON");

  currentLanguage.value = "ru";
  assert.equal(t("Switcher JSON"), "JSON Switcher");
  assert.equal(t("Token JSON"), "JSON токена");

  currentLanguage.value = "zh-TW";
  assert.equal(t("Switcher JSON"), "Switcher JSON");
  assert.equal(t("Token JSON"), "Token JSON");
});

test("API 服务绑定停服提示支持全部界面语言", () => {
  const message = "本次确认会先清空 API 服务中的现有账号，再写入所选账号。OAuth 账号会写入认证目录，API Key 账号会写入 CLIProxyAPI 上游配置。绑定期间服务会短暂停止并自动重启。";

  currentLanguage.value = "en";
  assert.match(t(message), /briefly stops and restarts automatically/);

  currentLanguage.value = "ru";
  assert.match(t(message), /автоматически перезапустится/);

  currentLanguage.value = "zh-TW";
  assert.match(t(message), /綁定期間服務會短暫停止並自動重新啟動/);
});
