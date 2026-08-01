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
