import test, { afterEach } from "node:test";
import assert from "node:assert/strict";
import { computed } from "vue";
import {
  calendarDateKey, calendarDayStart, currentTimeZone, effectiveTimeZone, formatAppDateTime,
  formatWallTime, normalizeTimeZone, parseWallTime, setTimeZone, supportedTimeZones,
  systemTimeZone, timeZoneOffsetLabel, usageCalendarRange,
} from "../src/timeZone.ts";
import { formatLocalScheduleInput, parseLocalScheduleInput } from "../src/services/resetScheduleEntry.ts";
import { formatResetDateTime } from "../src/services/resetUiState.ts";

const original = currentTimeZone.value;
const originalSystem = systemTimeZone.value;
afterEach(() => { setTimeZone(original); systemTimeZone.value = originalSystem; });

test("默认跟随系统，并在系统时区变化时响应；手动选择不受系统变化影响", () => {
  setTimeZone("system");
  systemTimeZone.value = "Asia/Shanghai";
  assert.equal(effectiveTimeZone.value, "Asia/Shanghai");
  systemTimeZone.value = "America/Los_Angeles";
  assert.equal(effectiveTimeZone.value, "America/Los_Angeles");
  setTimeZone("Asia/Kathmandu");
  systemTimeZone.value = "Europe/London";
  assert.equal(effectiveTimeZone.value, "Asia/Kathmandu");
  for (const value of ["", "invalid/zone", null, "GMT+8"]) assert.equal(normalizeTimeZone(value), "system");
  assert.ok(supportedTimeZones().includes("Asia/Shanghai"));
});

test("统一格式化具有响应性，跨日转换和半小时、45 分钟偏移准确", () => {
  const instant = Date.parse("2026-10-26T00:15:30Z");
  const display = computed(() => formatWallTime(instant, true));
  setTimeZone("America/Los_Angeles");
  assert.equal(display.value, "2026/10/25 17:15:30");
  setTimeZone("Asia/Shanghai");
  assert.equal(display.value, "2026/10/26 08:15:30");
  assert.match(formatAppDateTime(instant, { hour: "2-digit", minute: "2-digit", hour12: false }, "en-US"), /08:15/);
  assert.match(formatResetDateTime(instant, "zh-CN"), /08:15/);
  assert.equal(timeZoneOffsetLabel("Asia/Kolkata", instant), "GMT+5:30");
  assert.equal(timeZoneOffsetLabel("Asia/Kathmandu", instant), "GMT+5:45");
  assert.equal(formatWallTime(instant, true, "/", "Asia/Kathmandu"), "2026/10/26 06:00:30");
});

test("预约输入在所选时区解析，保持实际执行时间并拒绝无效日期及夏令时缺口", () => {
  setTimeZone("Asia/Shanghai");
  const instant = Date.parse("2026-10-26T00:15:00Z");
  assert.equal(formatLocalScheduleInput(instant), "2026-10-26 08:15");
  assert.equal(parseLocalScheduleInput("2026-10-26 08:15"), instant);
  assert.equal(parseWallTime("2026-02-30 10:00"), null);
  assert.equal(parseWallTime("2026-03-08 02:30", "America/Los_Angeles"), null);
  assert.equal(parseWallTime("2026-11-01 01:30", "America/Los_Angeles"), Date.parse("2026-11-01T08:30:00Z"));
  assert.equal(formatAppDateTime("bad date"), "—");
});

test("今天、昨天和上月按选定时区的日历边界筛选，正确处理夏令时长短日", () => {
  const now = Date.parse("2026-10-26T00:15:00Z");
  const la = usageCalendarRange("today", now, "America/Los_Angeles");
  const cn = usageCalendarRange("today", now, "Asia/Shanghai");
  assert.equal(la.start.toISOString(), "2026-10-25T07:00:00.000Z");
  assert.equal(cn.start.toISOString(), "2026-10-25T16:00:00.000Z");
  const previousMonth = usageCalendarRange("lastMonth", now, "Asia/Shanghai");
  assert.equal(previousMonth.start.toISOString(), "2026-08-31T16:00:00.000Z");
  assert.equal(previousMonth.end.toISOString(), "2026-09-30T15:59:59.999Z");
  for (const [day, hours] of [["2026-03-09", 23], ["2026-11-02", 25]] as const) {
    const range = usageCalendarRange("yesterday", Date.parse(`${day}T18:00:00Z`), "America/Los_Angeles");
    assert.equal(range.end.getTime() + 1 - range.start.getTime(), hours * 3_600_000);
  }
  const midnightGap = calendarDayStart("2018-11-04", "America/Sao_Paulo");
  assert.equal(calendarDateKey(midnightGap, "America/Sao_Paulo"), "2018-11-04");
  assert.equal(formatWallTime(midnightGap, false, "/", "America/Sao_Paulo"), "2018/11/04 01:00");
});
