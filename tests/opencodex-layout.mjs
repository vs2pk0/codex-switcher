import assert from 'node:assert/strict';
import { mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// Run against npm run dev. The fixture denies all mutating IPC.
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || 'playwright');
const browser = await chromium.launch({ channel: 'chrome', headless: true });
const output = await mkdtemp(join(tmpdir(), 'opencodex-layout-'));
const page = await browser.newPage({ viewport: { width: 1280, height: 1080 }, deviceScaleFactor: 1 });
const errors = [];
page.on('pageerror', error => errors.push(error.message));
const bounds = selector => page.locator(selector).boundingBox();
try {
  await page.goto('http://127.0.0.1:5173/tests/fixtures/opencodex-layout.html');
  await page.getByRole('button', { name: '版本管理', exact: true }).click();
  await page.locator('.local-version-row').first().waitFor();
  assert.equal(await page.locator('.local-version-row').count(), 3);
  assert.equal(await page.locator('.local-version-row').first().getByRole('button', { name: '删除' }).isDisabled(), true);
  const actions = await page.locator('.local-version-actions').evaluateAll(elements => elements.map(element => element.getBoundingClientRect().right));
  assert.equal(new Set(actions).size, 1, 'version actions share one right edge');
  await page.screenshot({ path: join(output, 'versions.png'), fullPage: true });
  for (const width of [760, 480]) {
    await page.setViewportSize({ width, height: 1080 });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true, 'version page fits narrow width');
    await page.screenshot({ path: join(output, `versions-${width}.png`), fullPage: true });
  }
  await page.setViewportSize({ width: 1280, height: 1080 });
  await page.getByRole('button', { name: '数据传输', exact: true }).click();
  const fields = await page.locator('.transfer-field').evaluateAll(elements => elements.map(element => ({ width: element.getBoundingClientRect().width, top: element.getBoundingClientRect().top })));
  assert.equal(fields[0].width, fields[1].width);
  assert.equal(fields[0].top, fields[1].top);
  const radio = await bounds('.mode-option input >> nth=0');
  const label = await bounds('.mode-option strong >> nth=0');
  assert.ok(Math.abs(radio.y - label.y) < 5, 'radio and label align horizontally');
  assert.equal(await page.getByRole('checkbox').isDisabled(), true);
  await page.getByRole('button', { name: '预览传输' }).click();
  await page.locator('.transfer-preview table').waitFor();
  assert.equal(await page.getByRole('button', { name: '执行传输' }).isEnabled(), true);
  await page.screenshot({ path: join(output, 'transfer.png'), fullPage: true, animations: 'disabled' });
  await page.getByRole('button', { name: '执行传输' }).click();
  await page.getByRole('button', { name: '取消', exact: true }).click();
  assert.equal(await page.getByRole('button', { name: '执行传输' }).isEnabled(), true, 'cancel releases operation lock');
  await page.locator('input[value="overwrite"]').check();
  assert.equal(await page.getByRole('button', { name: '执行传输' }).isDisabled(), true, 'changing mode invalidates preview');
  await page.locator('.transfer-history .arco-checkbox').click();
  await page.locator('input[value="merge"]').check();
  assert.equal(await page.getByRole('checkbox').isChecked(), false);
  for (const width of [760, 480]) {
    await page.setViewportSize({ width, height: 1080 });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true, 'no horizontal page overflow');
    await page.screenshot({ path: join(output, `transfer-${width}.png`), fullPage: true });
  }
  await page.setViewportSize({ width: 1280, height: 1080 });
  await page.getByRole('button', { name: '图片模型', exact: true }).click();
  await page.getByText('直接修改 OpenCodex 描述器', { exact: true }).waitFor();
  assert.equal(await page.locator('.vision-sidecar-editor .arco-select').count(), 2);
  // allow-search 模式下 input 只承载搜索文本，选中值显示在 .arco-select-view-value
  await page.locator('.vision-sidecar-editor .arco-select-view-value').first().filter({ hasText: 'Zc/qwen3.8-max' }).waitFor();
  assert.equal(await page.locator('.vision-sidecar-editor .arco-switch-checked').count(), 1);
  // 两个 select 必须保留标签关联（label 包裹），同时 label 的激活行为需被取消，否则不带 allow-search 的下拉会被二次 click 立即关闭
  // arco 有值时会把内部 input 收成 position:absolute; width:0（仍在无障碍树中但不可点击），所以这里只校验标签关联存在，点击走可见的 .arco-select
  assert.equal(await page.getByLabel(/^模型/).count(), 1, 'model select keeps its label');
  assert.equal(await page.getByLabel(/^执行后端/).count(), 1, 'backend select keeps its label');
  await page.locator('.vision-sidecar-editor .arco-select').nth(1).click();
  await page.locator('.arco-select-dropdown:visible').waitFor();
  assert.deepEqual(await page.locator('.arco-select-dropdown:visible .arco-select-option').allTextContents(), ['OpenAI 接口直连', 'Anthropic 接口直连', '按 Provider 路由转发']);
  await page.keyboard.press('Escape');
  await page.locator('.arco-select-dropdown:visible').waitFor({ state: 'hidden' });
  assert.equal(await page.getByRole('switch', { name: '启用图片描述模型' }).count(), 1);
  await page.screenshot({ path: join(output, 'vision-sidecar.png'), fullPage: true, animations: 'disabled' });
  await page.getByRole('button', { name: '设置', exact: true }).click();
  await page.getByText('维护与恢复', { exact: true }).waitFor();
  assert.equal(await page.locator('.settings-card').count(), 3);
  assert.equal(await page.locator('.settings-card-foot code').textContent(), '15800');
  await page.getByRole('button', { name: '扫描', exact: true }).waitFor();
  await page.locator('.migration-empty').waitFor();
  assert.equal(await page.getByRole('button', { name: '恢复原生 Codex' }).count(), 1);
  assert.equal(await page.getByRole('button', { name: '卸载 OpenCodex' }).count(), 1);
  await page.screenshot({ path: join(output, 'settings.png'), fullPage: true, animations: 'disabled' });
  for (const width of [760, 480]) {
    await page.setViewportSize({ width, height: 1080 });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true, 'settings page fits narrow width');
    await page.screenshot({ path: join(output, `settings-${width}.png`), fullPage: true });
  }
  await page.setViewportSize({ width: 1280, height: 1080 });
  await page.goto('http://127.0.0.1:5173/tests/fixtures/opencodex-layout.html?empty');
  await page.locator('.engine-setup-notice').waitFor();
  assert.equal(await page.getByText('未同步', { exact: true }).count(), 1);
  assert.equal(await page.getByText('旧版', { exact: false }).count(), 0);
  await page.goto('http://127.0.0.1:5173/tests/fixtures/opencodex-layout.html?stopped');
  await page.getByRole('button', { name: '版本管理', exact: true }).click();
  const remove = page.locator('.local-version-row').getByRole('button', { name: '删除', exact: true });
  await remove.waitFor();
  assert.equal(await remove.isEnabled(), true, 'last stopped Engine is removable');
  await remove.click();
  await page.getByText('这是最后一个 Engine', { exact: false }).waitFor();
  await page.getByRole('button', { name: '取消', exact: true }).click();
  assert.deepEqual(errors, []);
  console.log(JSON.stringify({ passed: true, screenshots: output }, null, 2));
} finally {
  await browser.close();
}
