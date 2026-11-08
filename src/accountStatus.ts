import type { CodexAccount } from "./types/codex";

export function canRefreshAccountToken(account: CodexAccount): boolean {
  return account.auth_mode !== "apikey"
    && !account.openai_api_key && !account.openaiApiKey
    && Boolean(account.tokens.refresh_token?.trim());
}

export function isAccountSubscriptionInactive(
  account: CodexAccount,
  referenceTime = Date.now(),
): boolean {
  return account.subscription_is_active === false
    || isSubscriptionExpired(account.subscription_active_until, referenceTime);
}

export function subscriptionUntil(
  account: CodexAccount,
  tokenFallback?: string,
): string | undefined {
  if (account.subscription_is_active !== undefined) {
    return account.subscription_is_active ? account.subscription_active_until : undefined;
  }
  return account.subscription_active_until || tokenFallback;
}

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
