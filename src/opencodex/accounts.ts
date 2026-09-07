import type { OpenCodexSwitcherAccount } from "./types";

export function filterMigrationAccounts(
  accounts: OpenCodexSwitcherAccount[], query: string, status: string, plan: string,
): OpenCodexSwitcherAccount[] {
  const search = query.trim().toLowerCase();
  return accounts.filter((account) =>
    (!status || account.status === status)
    && (!plan || (account.plan || "__unknown__") === plan)
    && (!search || [account.email, account.sourceId, account.targetAccountId]
      .some((value) => value.toLowerCase().includes(search))),
  );
}

export function toggleVisibleAccounts(
  selected: string[], visible: OpenCodexSwitcherAccount[], checked: boolean,
): string[] {
  const ids = visible.filter((account) => account.eligible || account.deletable).map((account) => account.sourceId);
  if (checked) return [...new Set([...selected, ...ids])];
  const visibleIds = new Set(ids);
  return selected.filter((id) => !visibleIds.has(id));
}
