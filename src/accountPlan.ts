import type { CodexAccount } from "./types/codex";

export function normalizeAuthFilePlan(value?: string): "prolite" | "promax" | undefined {
  const normalized = (value || "").trim().toLowerCase().replace(/[-_\s]+/g, "")
    .replace(/^(?:chatgpt|codex)/, "");
  if (["prolite", "pro5x"].includes(normalized)) return "prolite";
  if (["promax", "pro20x"].includes(normalized)) return "promax";
  return undefined;
}

export function accountProPlan(account: CodexAccount): "prolite" | "promax" | undefined {
  return normalizeAuthFilePlan(account.auth_file_plan_type)
    || normalizeAuthFilePlan(account.plan_type);
}

export function proPlanLabel(account: CodexAccount): string {
  const plan = accountProPlan(account);
  return plan === "prolite" ? "PRO 5X" : plan === "promax" ? "PRO 20X" : "PRO";
}
