export function hasAvailableResetCredit<T>(
  records: readonly T[],
  isAvailable: (record: T) => boolean,
): boolean {
  return records.some(isAvailable);
}

export function formatResetDateTime(timestamp: number, locale: string): string {
  if (!Number.isFinite(timestamp)) return "—";
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) return "—";
  return new Intl.DateTimeFormat(locale, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

export interface ResetCreditRowActionState {
  consumeDisabled: boolean;
  scheduleDisabled: boolean;
  scheduleAction: "create" | "view";
}

export function resetCreditRowActionState(
  available: boolean,
  hasScheduledReset: boolean,
  busy: boolean,
): ResetCreditRowActionState {
  const scheduleAction = hasScheduledReset ? "view" : "create";
  return {
    consumeDisabled: !available || hasScheduledReset || busy,
    scheduleDisabled: !available || (scheduleAction === "create" && busy),
    scheduleAction,
  };
}

export function beginPendingItem(
  activeIds: readonly string[],
  itemId: string,
): string[] | null {
  if (activeIds.includes(itemId)) return null;
  return [...activeIds, itemId];
}

export function finishPendingItem(
  activeIds: readonly string[],
  itemId: string,
): string[] {
  return activeIds.filter((id) => id !== itemId);
}
