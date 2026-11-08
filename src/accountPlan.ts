import type { CodexAccount } from "./types/codex";

export function normalizeAuthFilePlan(value?: string): "prolite" | "promax" | undefined {
  const normalized = (value || "").trim().toLowerCase().replace(/[-_\s]+/g, "")
    .replace(/^(?:chatgpt|codex)/, "");
  if (["prolite", "pro5", "pro5x"].includes(normalized)) return "prolite";
  if (["promax", "pro20", "pro20x"].includes(normalized)) return "promax";
  return undefined;
}

export function accountProPlan(account: CodexAccount): "prolite" | "promax" | undefined {
  return normalizeAuthFilePlan(account.auth_file_plan_type)
    || normalizeAuthFilePlan(account.plan_type)
    // The standard Pro entitlement is returned as `pro`; Lite has its own
    // entitlement. Resolve explicit tiers first so a generic field cannot hide 5X.
    || (/^(?:(?:chatgpt|codex)[-_\s]*)?pro$/i.test((account.plan_type || "").trim())
      ? "promax" : undefined);
}

export function proPlanLabel(account: CodexAccount): string {
  const plan = accountProPlan(account);
  return plan === "prolite" ? "PRO 5X" : plan === "promax" ? "PRO 20X" : "PRO";
}
