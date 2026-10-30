import assert from "node:assert/strict";
import test from "node:test";
import { flattenModels, getCommonModelKeys, normalizeModelsDevModelId, resolveModelsDevSelection, toModelPricing } from "../src/modelsDevPricing.ts";

test("models.dev import normalizes IDs, preserves cache prices and ignores invalid/non-text records", () => {
  const entries = flattenModels({openai: {name: "OpenAI", models: {
    "vendor/GPT-6-Astra:fast": {name: "Astra", release_date: "2026-10-01", cost: {input: 10, output: 50, cache_read: 1, cache_write: 12.5}},
    "gpt-audio": {cost:{input: 1, output: 2}},
    "gpt-image": {cost:{input: 1, output: 2}, modalities:{output:["image"]}},
    "bad": {cost:{input: -1, output: 1}},
    "bad-nan": {cost:{input: NaN, output: 1}},
    "bad-infinity": {cost:{input: 1, output: Infinity}},
    "no-cost": {},
  }}});
  assert.equal(entries.length, 1);
  assert.equal(normalizeModelsDevModelId("anthropic/Claude-Opus@4[1m]"), "claude-opus-4");
  assert.deepEqual(toModelPricing(entries), [{modelId: "gpt-6-astra", displayName: "Astra", inputCostPerMillion: "10", outputCostPerMillion: "50", cacheReadCostPerMillion: "1", cacheCreationCostPerMillion: "12.5"}]);
});

test("common selection is bounded and explicit selections override exclusions", () => {
  const models = Object.fromEntries(Array.from({length: 8}, (_, i) => [`gpt-${i}`, {release_date:`2026-10-${10 + i}`, cost:{input:1, output:2}}]));
  const entries = flattenModels({openai:{models}, reseller:{models}});
  const common = getCommonModelKeys(entries);
  assert.equal(common.size, 6);
  assert.ok(common.has("openai/gpt-7"));
  assert.ok(!common.has("reseller/gpt-7"));
  const config = {autoSyncEnabled:false, includeCommonModels:true, selectedModelKeys:["openai/gpt-0"], excludedCommonModelKeys:["openai/gpt-7"], lastSyncAt:null,lastSyncError:null};
  const resolved = resolveModelsDevSelection(entries, config);
  assert.equal(resolved.length, 6);
  assert.ok(resolved.some(e => e.key === "openai/gpt-0"));
  assert.ok(!resolved.some(e => e.key === "openai/gpt-7"));
  config.selectedModelKeys.push("openai/gpt-7");
  assert.equal(resolveModelsDevSelection(entries, config).length, 7);
  assert.equal(toModelPricing(entries).length, 8);
});
