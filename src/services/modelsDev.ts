import { invoke } from "@tauri-apps/api/core";
import { ref, shallowRef } from "vue";
import { flattenModels, resolveModelsDevSelection, toModelPricing, type ModelsDevEntry, type ModelsDevResponse, type ModelsDevSyncConfig } from "../modelsDevPricing";
import type { CodexUsagePricing } from "./usage";

export interface ModelsDevSyncState { config: ModelsDevSyncConfig; configPath: string }
export const modelsDevSyncState = shallowRef<ModelsDevSyncState | null>(null);
export const modelsDevSyncing = ref(false);
export const modelsDevPricingRevision = ref(0);
export const modelsDevSyncInterval = 6 * 60 * 60 * 1000;
let cached: { entries: ModelsDevEntry[]; at: number } | null = null;
let fetching: Promise<ModelsDevEntry[]> | null = null;
let syncing: Promise<number | null> | null = null;

export async function loadModelsDevConfig(): Promise<ModelsDevSyncState> {
  const state = await invoke<ModelsDevSyncState>("codex_get_models_dev_sync_config");
  modelsDevSyncState.value = state;
  return state;
}
export async function saveModelsDevConfig(config: ModelsDevSyncConfig): Promise<void> {
  modelsDevSyncState.value = await invoke("codex_update_models_dev_sync_config", { config });
}
export function fetchModelsDevEntries(force = false): Promise<ModelsDevEntry[]> {
  if (!force && cached && Date.now() - cached.at < 60 * 60 * 1000) return Promise.resolve(cached.entries);
  if (fetching) return fetching;
  fetching = invoke<ModelsDevResponse>("codex_fetch_models_dev").then((data) => {
    const entries = flattenModels(data);
    if (!entries.length) throw new Error("models.dev 未返回可导入的模型价格");
    cached = { entries, at: Date.now() };
    return entries;
  }).finally(() => { fetching = null; });
  return fetching;
}
export async function importModelsDevPrices(prices: CodexUsagePricing[]): Promise<number> {
  const changed = await invoke<number>("codex_update_model_pricing_batch", { prices });
  if (changed > 0) modelsDevPricingRevision.value++;
  return changed;
}
export function syncModelsDevPricing(force = false): Promise<number | null> {
  if (syncing) return syncing;
  syncing = (async () => {
    let state = await loadModelsDevConfig();
    if (!force && (!state.config.autoSyncEnabled || (state.config.lastSyncAt !== null && Date.now() - state.config.lastSyncAt < modelsDevSyncInterval))) return null;
    modelsDevSyncing.value = true;
    try {
      const entries = await fetchModelsDevEntries(force);
      state = await loadModelsDevConfig();
      if (!force && !state.config.autoSyncEnabled) return null;
      const prices = toModelPricing(resolveModelsDevSelection(entries, state.config));
      const changed = prices.length ? await importModelsDevPrices(prices) : 0;
      modelsDevSyncState.value = await invoke("codex_record_models_dev_sync_result", { syncedAt: Date.now(), error: null });
      return changed;
    } catch (error) {
      try { modelsDevSyncState.value = await invoke("codex_record_models_dev_sync_result", { syncedAt: null, error: String(error) }); } catch { /* Keep the original error. */ }
      throw error;
    } finally { modelsDevSyncing.value = false; }
  })().finally(() => { syncing = null; });
  return syncing;
}

export function startModelsDevAutoSync(): () => void {
  const run = () => { void syncModelsDevPricing().catch(() => { /* Persisted error appears in pricing settings. */ }); };
  run();
  const interval = window.setInterval(run, modelsDevSyncInterval);
  window.addEventListener("focus", run);
  return () => { window.clearInterval(interval); window.removeEventListener("focus", run); };
}
