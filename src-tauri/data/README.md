# Model pricing presets

`model-pricing.json` contains all 232 built-in model prices from [CC Switch](https://github.com/farion1231/cc-switch), revision `7569b27af4f485172950fb6989f5643163842bd1`, retrieved on 2026-10-30.

Source: `src-tauri/src/database/schema.rs`, `seed_model_pricing`, followed by its conditional `repair_current_model_pricing` corrections. Prices are strings in USD per 1 million tokens. This is a bundled snapshot, not a live price feed; provider-specific, long-context and speed-tier surcharges are not represented.

`model-pricing-v1.json` freezes this app's previous built-in GPT/Codex prices. The version 2 migration uses it to update unchanged built-in prices while preserving custom entries and prices. Migration runs once; deleting a rule afterwards does not recreate it. Resetting defaults explicitly replaces all prices with the bundled snapshot.

`model-pricing-v3.json` adds eight rates checked on 2026-11-20 from [OpenAI](https://developers.openai.com/api/docs/pricing), [Anthropic](https://platform.claude.com/docs/en/about-claude/pricing) and [Google](https://aistudio.google.com/docs/pricing). Rates use Standard short-context API prices, 30-minute OpenAI cache writes, and five-minute Anthropic cache writes. Gemini 4 Carbon uses the introductory rate effective through 2026-11-30. The version 3 migration adds missing new models and keeps existing prices.

The models.dev catalog selection, price formatting and Codex session request timing also adapt CC Switch code at the same revision (`src/lib/modelsDevPricing.ts`, `src/components/usage/format.ts`, `src-tauri/src/services/session_usage_codex.rs`). Session speed is estimated from timestamps, includes first-token waiting, and excludes requests with fewer than 200 output tokens or less than 1 second of duration. Aggregate speed divides eligible total output by eligible total duration.

Live optional price synchronization fetches the public [models.dev API](https://models.dev/api.json); it is disabled by default and only updates selected models.

CC Switch license (applies to the imported pricing data and adapted code):

```text
MIT License

Copyright (c) 2025 Jason Young

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
