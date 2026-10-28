import { computed, ref } from "vue";
import { currentLocale } from "./i18n.ts";

export const SYSTEM_TIME_ZONE = "system";
const storageKey = "codex-switcher:time-zone";

export function normalizeTimeZone(value: unknown): string {
  if (typeof value !== "string" || !value.trim() || value === SYSTEM_TIME_ZONE) return SYSTEM_TIME_ZONE;
  try {
    new Intl.DateTimeFormat("en", { timeZone: value.trim() }).format(0);
    return value.trim();
  } catch {
    return SYSTEM_TIME_ZONE;
  }
}

function readPreference(): string {
  try { return normalizeTimeZone(globalThis.localStorage?.getItem(storageKey)); }
  catch { return SYSTEM_TIME_ZONE; }
}

export const currentTimeZone = ref(readPreference());
export const systemTimeZone = ref(Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC");
export const effectiveTimeZone = computed(() => currentTimeZone.value === SYSTEM_TIME_ZONE
  ? systemTimeZone.value : currentTimeZone.value);

export function setTimeZone(value: unknown): void {
  currentTimeZone.value = normalizeTimeZone(value);
  try { globalThis.localStorage?.setItem(storageKey, currentTimeZone.value); } catch { /* Settings remain authoritative. */ }
}

export function trackSystemTimeZone(): () => void {
  const refresh = () => { systemTimeZone.value = Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC"; };
  refresh();
  const timer = window.setInterval(refresh, 30_000);
  window.addEventListener("focus", refresh);
  document.addEventListener("visibilitychange", refresh);
  return () => {
    window.clearInterval(timer);
    window.removeEventListener("focus", refresh);
    document.removeEventListener("visibilitychange", refresh);
  };
}

export function formatAppDateTime(
  value: Date | number | string,
  options: Intl.DateTimeFormatOptions = { year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" },
  locale = currentLocale(),
): string {
  const date = value instanceof Date ? value : new Date(value);
  if (!Number.isFinite(date.getTime())) return "—";
  return new Intl.DateTimeFormat(locale, { ...options, timeZone: effectiveTimeZone.value }).format(date);
}

export interface CalendarParts { year: number; month: number; day: number; hour: number; minute: number; second: number }
const partFormatters = new Map<string, Intl.DateTimeFormat>();

export function calendarParts(value: Date | number, timeZone = effectiveTimeZone.value): CalendarParts {
  let formatter = partFormatters.get(timeZone);
  if (!formatter) {
    formatter = new Intl.DateTimeFormat("en-CA", {
      timeZone, calendar: "gregory", numberingSystem: "latn", hourCycle: "h23",
      year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", second: "2-digit",
    });
    partFormatters.set(timeZone, formatter);
  }
  const parts = Object.fromEntries(formatter.formatToParts(value).filter(part => part.type !== "literal").map(part => [part.type, Number(part.value)]));
  return parts as unknown as CalendarParts;
}

const pad = (value: number) => String(value).padStart(2, "0");
export function calendarDateKey(value: Date | number, timeZone = effectiveTimeZone.value): string {
  const p = calendarParts(value, timeZone);
  return `${p.year}-${pad(p.month)}-${pad(p.day)}`;
}

export function formatWallTime(value: Date | number, seconds = false, separator = "/", timeZone = effectiveTimeZone.value): string {
  if (!Number.isFinite(new Date(value).getTime())) return "";
  const p = calendarParts(value, timeZone);
  return `${p.year}${separator}${pad(p.month)}${separator}${pad(p.day)} ${pad(p.hour)}:${pad(p.minute)}${seconds ? `:${pad(p.second)}` : ""}`;
}

function utcCalendarTimestamp(p: CalendarParts): number {
  const date = new Date(0);
  date.setUTCFullYear(p.year, p.month - 1, p.day);
  date.setUTCHours(p.hour, p.minute, p.second, 0);
  return date.getTime();
}

function sameParts(left: CalendarParts, right: CalendarParts): boolean {
  return (["year", "month", "day", "hour", "minute", "second"] as const).every(key => left[key] === right[key]);
}

// Reject nonexistent wall times; choose the first occurrence when DST repeats an hour.
export function parseWallTime(value: string, timeZone = effectiveTimeZone.value): number | null {
  const match = /^(\d{4})[-/](\d{2})[-/](\d{2}) (\d{2}):(\d{2})(?::(\d{2}))?$/.exec(value);
  if (!match) return null;
  const [year, month, day, hour, minute, second] = match.slice(1).map(item => Number(item || 0));
  const target = { year, month, day, hour, minute, second };
  const naive = utcCalendarTimestamp(target);
  if (!sameParts(calendarParts(naive, "UTC"), target)) return null;
  const offsets = new Set<number>();
  for (const hours of [-48, -24, -12, 0, 12, 24, 48]) {
    const probe = naive + hours * 3_600_000;
    offsets.add(utcCalendarTimestamp(calendarParts(probe, timeZone)) - probe);
  }
  const candidates = [...offsets].map(offset => naive - offset)
    .filter(timestamp => sameParts(calendarParts(timestamp, timeZone), target));
  return candidates.length ? Math.min(...candidates) : null;
}

function shiftedDate(parts: CalendarParts, days = 0, months = 0): string {
  const date = new Date(utcCalendarTimestamp({ ...parts, day: parts.day + days, month: parts.month + months, hour: 12, minute: 0, second: 0 }));
  return calendarDateKey(date, "UTC");
}

export function calendarDayStart(dateKey: string, timeZone = effectiveTimeZone.value): number {
  // Some zones advance their clocks at midnight. Use the first existing minute of that day.
  for (let minute = 0; minute < 24 * 60; minute++) {
    const timestamp = parseWallTime(`${dateKey} ${pad(Math.floor(minute / 60))}:${pad(minute % 60)}`, timeZone);
    if (timestamp !== null) return timestamp;
  }
  throw new RangeError(`Date does not exist in ${timeZone}: ${dateKey}`);
}

export function nextCalendarDayStart(now = Date.now(), timeZone = effectiveTimeZone.value): number {
  return calendarDayStart(shiftedDate(calendarParts(now, timeZone), 1), timeZone);
}

export function usageCalendarRange(preset: string, now = Date.now(), timeZone = effectiveTimeZone.value): { start: Date; end: Date } {
  const p = calendarParts(now, timeZone);
  let startKey = calendarDateKey(now, timeZone);
  let end = now;
  if (preset === "yesterday" || preset === "beforeYesterday") {
    const offset = preset === "yesterday" ? -1 : -2;
    startKey = shiftedDate(p, offset);
    end = calendarDayStart(shiftedDate(p, offset + 1), timeZone) - 1;
  } else if (preset === "thisMonth" || preset === "lastMonth") {
    const first = { ...p, day: 1 };
    startKey = shiftedDate(first, 0, preset === "lastMonth" ? -1 : 0);
    if (preset === "lastMonth") end = calendarDayStart(shiftedDate(first), timeZone) - 1;
  }
  return { start: new Date(calendarDayStart(startKey, timeZone)), end: new Date(end) };
}

export function timeZoneOffsetLabel(zone: string, now = Date.now()): string {
  const minutes = Math.round((utcCalendarTimestamp(calendarParts(now, zone)) - Math.floor(now / 1000) * 1000) / 60_000);
  if (!minutes) return "GMT";
  return `GMT${minutes < 0 ? "−" : "+"}${Math.floor(Math.abs(minutes) / 60)}${minutes % 60 ? `:${pad(Math.abs(minutes) % 60)}` : ""}`;
}

export function supportedTimeZones(): string[] {
  const api = Intl as typeof Intl & { supportedValuesOf?: (key: string) => string[] };
  return [...new Set(["UTC", ...(api.supportedValuesOf?.("timeZone") ?? [
    "America/Los_Angeles", "America/New_York", "America/Chicago", "America/Denver", "America/Sao_Paulo",
    "Europe/London", "Europe/Paris", "Europe/Berlin", "Europe/Moscow", "Asia/Dubai", "Asia/Kolkata",
    "Asia/Kathmandu", "Asia/Bangkok", "Asia/Shanghai", "Asia/Hong_Kong", "Asia/Taipei", "Asia/Singapore",
    "Asia/Tokyo", "Asia/Seoul", "Australia/Sydney", "Pacific/Auckland",
  ])])];
}
