import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('全局顶栏只在账号总览渲染，且不再显示应用名标题', () => {
  const header = readFileSync(new URL('../src/components/AppHeader.vue', import.meta.url), 'utf8');
  assert.match(header, /<header v-if="activeView === 'accounts'" class="topbar">/);
  assert.doesNotMatch(header, /<h1>Codex Switcher<\/h1>/);
});

test('侧边栏提供推送入口，重置记录从设置页进入', () => {
  const header = readFileSync(new URL('../src/components/AppHeader.vue', import.meta.url), 'utf8');
  assert.match(header, /@click="\$emit\('switch-view', 'pushSettings'\)"/);
  assert.doesNotMatch(header, /@click="\$emit\('switch-view', 'resets'\)"/);
  const settings = readFileSync(new URL('../src/components/SettingsPanel.vue', import.meta.url), 'utf8');
  assert.match(settings, /emit\('open-reset-records'\)/);
  const app = readFileSync(new URL('../src/App.vue', import.meta.url), 'utf8');
  assert.match(app, /@open-reset-records="switchView\('resets'\)"/);
});

test('Engine 完成进度收起，最后版本的数据清理需要显式授权', () => {
  const panel = readFileSync(new URL('../src/opencodex/OpenCodexPanel.vue', import.meta.url), 'utf8');
  assert.match(panel, /engineProgress && engineProgress.stage !== 'complete'/);
  assert.match(panel, /deleteOpenCodexEngine\(version, instanceId, fullRemoval\)/);
  const backend = readFileSync(new URL('../src-tauri/src/opencodex/backend.rs', import.meta.url), 'utf8');
  assert.match(backend, /if !request.remove_data/);
});
