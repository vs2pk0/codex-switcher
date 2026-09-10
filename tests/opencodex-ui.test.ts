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

test('图片模型页提供 Codex Image Gen 上游设置，并通过 ocx config 写入 images.provider', () => {
  const panel = readFileSync(new URL('../src/opencodex/OpenCodexPanel.vue', import.meta.url), 'utf8');
  assert.match(panel, /class="vision-sidecar-editor image-gen-editor"/);
  assert.match(panel, /updateOpenCodexImageGenerationSettings\(\s*\{ provider, timeoutMs: timeoutSeconds \* 1000 \}/);
  const service = readFileSync(new URL('../src/opencodex/service.ts', import.meta.url), 'utf8');
  assert.match(service, /invoke\("opencodex_get_image_generation_settings"/);
  assert.match(service, /invoke\("opencodex_update_image_generation_settings"/);
  const backend = readFileSync(new URL('../src-tauri/src/opencodex/backend.rs', import.meta.url), 'utf8');
  // 镜像提供方不暴露聊天模型，且只在源提供方不是 openai-responses 时创建。
  assert.match(backend, /const IMAGE_MIRROR_SUFFIX: &str = "-images";/);
  assert.match(backend, /mirror\.insert\("liveModels"\.into\(\), Value::Bool\(false\)\)/);
  assert.match(backend, /fn update_image_generation_settings/);
  const lib = readFileSync(new URL('../src-tauri/src/lib.rs', import.meta.url), 'utf8');
  assert.match(lib, /opencodex::opencodex_update_image_generation_settings/);
});

test('会话页提供按实例查看与清理会话编辑备份的入口', () => {
  const panel = readFileSync(new URL('../src/components/SessionPanel.vue', import.meta.url), 'utf8');
  assert.match(panel, /<SessionEditBackupCard\s/);
  const card = readFileSync(new URL('../src/components/SessionEditBackupCard.vue', import.meta.url), 'utf8');
  // 四种清理范围都要经过确认弹窗。
  for (const scope of ['selected', 'instance', 'orphan', 'all']) {
    assert.match(card, new RegExp(`confirmDelete\\('${scope}'\\)`));
  }
  const service = readFileSync(new URL('../src/services/session.ts', import.meta.url), 'utf8');
  assert.match(service, /invoke\("list_session_edit_backups"/);
  assert.match(service, /invoke\("delete_session_edit_backups"/);
  const lib = readFileSync(new URL('../src-tauri/src/lib.rs', import.meta.url), 'utf8');
  assert.match(lib, /instances::list_session_edit_backups,/);
  assert.match(lib, /instances::delete_session_edit_backups,/);
});

test('回收站提供经确认后永久删除会话的入口', () => {
  const panel = readFileSync(new URL('../src/components/SessionPanel.vue', import.meta.url), 'utf8');
  assert.match(panel, /emit\('purge-sessions'\)/);
  const app = readFileSync(new URL('../src/App.vue', import.meta.url), 'utf8');
  assert.match(app, /@purge-sessions="handlePurgeSessions"/);
  assert.match(app, /function handlePurgeSessions[\s\S]*?Modal\.confirm\([\s\S]*?purgeSessionsFromTrashAcrossInstances/);
  const service = readFileSync(new URL('../src/services/session.ts', import.meta.url), 'utf8');
  assert.match(service, /invoke\("codex_purge_sessions_from_trash_across_instances"/);
  const lib = readFileSync(new URL('../src-tauri/src/lib.rs', import.meta.url), 'utf8');
  assert.match(lib, /codex_purge_sessions_from_trash_across_instances,/);
});

test('只有主窗口销毁才触发 API 服务等后台组件的退出逻辑', () => {
  const lib = readFileSync(new URL('../src-tauri/src/lib.rs', import.meta.url), 'utf8');
  // OpenCodex Web 管理子窗口关闭时不能把 API 服务标记为"应用正在退出"。
  assert.match(lib, /matches!\(event, tauri::WindowEvent::Destroyed\) && window\.label\(\) == "main"/);
});

test('Engine 完成进度收起，最后版本的数据清理需要显式授权', () => {
  const panel = readFileSync(new URL('../src/opencodex/OpenCodexPanel.vue', import.meta.url), 'utf8');
  assert.match(panel, /engineProgress && engineProgress.stage !== 'complete'/);
  assert.match(panel, /deleteOpenCodexEngine\(version, instanceId, fullRemoval\)/);
  const backend = readFileSync(new URL('../src-tauri/src/opencodex/backend.rs', import.meta.url), 'utf8');
  assert.match(backend, /if !request.remove_data/);
});
