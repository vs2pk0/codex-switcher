export type ResetScheduleEntryAction = "create" | "view";

export function resolveResetScheduleEntry(
  hasActiveSchedule: boolean,
): ResetScheduleEntryAction {
  return hasActiveSchedule ? "view" : "create";
}

function padLocalDatePart(value: number): string {
  return String(value).padStart(2, "0");
}

export function formatLocalScheduleInput(timestamp: number): string {
  if (!Number.isFinite(timestamp)) return "";
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) return "";
  return [
    `${date.getFullYear()}-${padLocalDatePart(date.getMonth() + 1)}-${padLocalDatePart(date.getDate())}`,
    `${padLocalDatePart(date.getHours())}:${padLocalDatePart(date.getMinutes())}`,
  ].join(" ");
}

export function parseLocalScheduleInput(value: string): number | null {
  const match = /^(\d{4})-(\d{2})-(\d{2}) (\d{2}):(\d{2})$/.exec(value);
  if (!match) return null;

  const [, yearText, monthText, dayText, hourText, minuteText] = match;
  const year = Number(yearText);
  const month = Number(monthText);
  const day = Number(dayText);
  const hour = Number(hourText);
  const minute = Number(minuteText);
  const date = new Date(year, month - 1, day, hour, minute, 0, 0);

  if (
    date.getFullYear() !== year ||
    date.getMonth() !== month - 1 ||
    date.getDate() !== day ||
    date.getHours() !== hour ||
    date.getMinutes() !== minute
  ) {
    return null;
  }

  return date.getTime();
}
