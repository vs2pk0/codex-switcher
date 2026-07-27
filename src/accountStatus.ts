export function isSubscriptionExpired(
  value?: string | number | null,
  referenceTime = Date.now(),
): boolean {
  if (value === undefined || value === null || value === "") return false;
  const numeric = typeof value === "number" ? value : Number(value);
  const date = Number.isFinite(numeric)
    ? new Date(numeric > 10_000_000_000 ? numeric : numeric * 1000)
    : new Date(String(value));
  return !Number.isNaN(date.getTime()) && date.getTime() <= referenceTime;
}
