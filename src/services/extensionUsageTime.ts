import { calendarDateKey, calendarDayStart, calendarParts, usageCalendarRange } from "../timeZone.ts";

export type UsagePeriod = "today" | "yesterday" | "thisMonth" | "lastMonth" | "custom";
const DAY = 86_400_000;
const MINUTE = 60_000;

/** UTC clock used only to step through calendar dates. */
function dateKey(utc: number): string {
  return new Date(utc).toISOString().slice(0, 10);
}

export function extensionUsageBounds(
  period: UsagePeriod,
  dates: readonly string[],
  now: number,
  timeZone: string,
): { start: number; end: number; buckets: number[]; hourly: boolean } {
  let start: number;
  let end: number;
  if (period === "custom") {
    const first = Date.parse(`${dates[0]}T00:00:00Z`);
    const last = Date.parse(`${dates[1]}T00:00:00Z`);
    if (!Number.isFinite(first) || !Number.isFinite(last) || last < first || last - first >= 90 * DAY) {
      throw new RangeError("请选择不超过 90 天的日期范围");
    }
    start = calendarDayStart(dateKey(first), timeZone);
    end = calendarDayStart(dateKey(last + DAY), timeZone);
  } else {
    const range = usageCalendarRange(period, now, timeZone);
    start = range.start.getTime();
    end = range.end.getTime() + 1;
  }
  const first = calendarDateKey(start, timeZone);
  const last = calendarDateKey(end - 1, timeZone);
  const hourly = first === last;
  const boundaries = [start];
  if (hourly) {
    let previous = calendarParts(start, timeZone);
    for (let time = start + MINUTE; time < end; time += MINUTE) {
      const parts = calendarParts(time, timeZone);
      // Include the clock change itself in zones with a partial-hour jump.
      if (parts.hour !== previous.hour || parts.minute <= previous.minute) boundaries.push(time);
      previous = parts;
    }
  } else {
    const firstDay = Date.parse(`${first}T00:00:00Z`);
    const lastDay = Date.parse(`${last}T00:00:00Z`);
    for (let day = firstDay + DAY; day <= lastDay; day += DAY) {
      try {
        boundaries.push(calendarDayStart(dateKey(day), timeZone));
      } catch (error) {
        // A calendar day skipped by a time zone has no bucket.
        if (!(error instanceof RangeError)) throw error;
      }
    }
  }
  const endSeconds = Math.ceil(end / 1000);
  return {
    start: Math.floor(start / 1000),
    end: endSeconds,
    buckets: [...boundaries.map((value) => Math.floor(value / 1000)), endSeconds],
    hourly,
  };
}
