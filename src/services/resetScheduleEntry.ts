import { formatWallTime, parseWallTime } from "../timeZone.ts";

export type ResetScheduleEntryAction = "create" | "view";

export function resolveResetScheduleEntry(
  hasActiveSchedule: boolean,
): ResetScheduleEntryAction {
  return hasActiveSchedule ? "view" : "create";
}

export function formatLocalScheduleInput(timestamp: number): string {
  return formatWallTime(timestamp, false, "-");
}

export function parseLocalScheduleInput(value: string): number | null {
  if (!/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/.test(value)) return null;
  return parseWallTime(value);
}
