import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('OpenCodex 与多开页不渲染重复的全局标题', () => {
  const header = readFileSync(new URL('../src/components/AppHeader.vue', import.meta.url), 'utf8');
  assert.match(header, /<header v-if="activeView !== 'openCodex' && activeView !== 'instances'" class="topbar">/);
});

test('Engine 完成进度收起，最后版本的数据清理需要显式授权', () => {
  const panel = readFileSync(new URL('../src/opencodex/OpenCodexPanel.vue', import.meta.url), 'utf8');
  assert.match(panel, /engineProgress && engineProgress.stage !== 'complete'/);
  assert.match(panel, /deleteOpenCodexEngine\(version, instanceId, fullRemoval\)/);
  const backend = readFileSync(new URL('../src-tauri/src/opencodex/backend.rs', import.meta.url), 'utf8');
  assert.match(backend, /if !request.remove_data/);
});
