<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { Message, Modal } from "@arco-design/web-vue";
import { instanceDisplayName, type CodexInstance } from "../services/instances";
import { transferOpenCodexData } from "./service";
const props = defineProps<{ instances: CodexInstance[]; targetId: string; disabled: boolean }>();
const emit = defineEmits<{ (event: "busy", value: boolean): void; (event: "complete"): void }>();
const source = ref(props.instances.find(i => i.id !== props.targetId)?.id || "");
const mode = ref<"merge" | "overwrite">("merge");
const history = ref(false);
const loading = ref(false);
const preview = ref<Awaited<ReturnType<typeof transferOpenCodexData>> | null>(null);
const previewError = ref("");
const backup = ref("");
const targetName = computed(() => {
  const instance = props.instances.find(i => i.id === props.targetId);
  return instance ? instanceDisplayName(instance) : "默认实例";
});
watch([source,mode,history], () => { preview.value = null; previewError.value = ""; if (mode.value === "merge") history.value = false; });
async function showPreview(): Promise<void> {
  loading.value = true; emit("busy",true);
  previewError.value = "";
  try { preview.value = await transferOpenCodexData(source.value,props.targetId,mode.value,history.value,false); }
  catch (e) { preview.value = null; previewError.value = String(e); Message.error(previewError.value); }
  finally { loading.value = false; emit("busy",false); }
}
function transfer(): void {
  if (!preview.value) return;
  const plan = { source: source.value, target: props.targetId, mode: mode.value, history: history.value, fingerprint: preview.value.fingerprint };
  emit("busy",true);
  Modal.warning({
    title: plan.mode === "overwrite" ? "确认覆盖目标数据" : "确认复制数据",
    content: `目标：${targetName.value}。执行时会暂停源和目标 OpenCodex，备份目标后传输配置和账号${plan.history ? "及用量历史" : ""}，完成后恢复原运行状态。`,
    hideCancel: false,
    onCancel: () => emit("busy",false),
    onClose: () => { if (!loading.value) emit("busy",false); },
    onOk: async () => {
      loading.value = true;
      try {
        const result = await transferOpenCodexData(plan.source,plan.target,plan.mode,plan.history,true,plan.fingerprint);
        backup.value = result.backupPath || ""; preview.value = null;
        Message.success(result.message); emit("complete");
      } catch (e) { Message.error(String(e)); }
      finally { loading.value = false; emit("busy",false); }
    },
  });
}
</script>
<template>
  <section class="transfer-card">
    <header class="transfer-heading"><div><h2>选择源与目标实例</h2><p>复制账号与配置；目标未安装时，一并复制并激活源实例的 Engine，无需额外下载。</p></div><span class="isolation-badge">独立存储</span></header>
    <div class="transfer-fields">
      <div class="transfer-field"><span class="transfer-field-icon"><icon-storage /></span><label for="transfer-source">源实例 <span>从此处复制数据</span></label><a-select id="transfer-source" v-model="source" :disabled="disabled || loading" placeholder="请选择源实例"><a-option v-for="instance in instances.filter(i => i.id !== targetId)" :key="instance.id" :value="instance.id">{{ instanceDisplayName(instance) }}</a-option></a-select></div>
      <span class="transfer-direction" aria-hidden="true">→</span>
      <div class="transfer-field"><span class="transfer-field-icon"><icon-storage /></span><label for="transfer-target">目标实例 <span>传输到此实例</span></label><a-input id="transfer-target" :model-value="targetName" readonly /></div>
    </div>
    <fieldset class="transfer-modes" :disabled="disabled || loading"><legend>传输方式</legend><div class="mode-grid">
      <label class="mode-option" :class="{ selected: mode === 'merge' }"><input v-model="mode" type="radio" value="merge" name="transfer-mode" /><span><strong>复制合并</strong><small>补充源数据，冲突时保留目标实例的数据。</small></span></label>
      <label class="mode-option" :class="{ selected: mode === 'overwrite' }"><input v-model="mode" type="radio" value="overwrite" name="transfer-mode" /><span><strong>覆盖配置和账号</strong><small>以源实例为准，替换目标的配置与账号。</small></span></label>
    </div></fieldset>
    <div class="transfer-history"><a-checkbox v-model="history" :disabled="disabled || loading || mode !== 'overwrite'">同时覆盖用量历史</a-checkbox><span>{{ mode === 'overwrite' ? '替换目标的 usage.jsonl，不合并历史记录' : '仅覆盖模式可选，合并时保留目标用量历史' }}</span></div>
    <aside class="transfer-note"><strong>传输前自动备份</strong><p>目标未安装或版本不同时，会复制源 Engine 并安全切换到同一版本。目录、端口、管理凭证和集成记录保持独立。执行时暂停双方服务，完成后恢复原运行状态；新实例保持停止，可直接启动。相同 OAuth 账号仍对应同一上游身份。</p></aside>
    <section class="transfer-preview" aria-live="polite"><h3>传输预览</h3>
      <div v-if="preview" class="preview-table-scroll"><table><thead><tr><th>数据类型</th><th>源实例</th><th>目标实例</th><th>传输后</th></tr></thead><tbody><tr v-for="row in preview.rows" :key="row.name"><td>{{ row.name }}</td><td>{{ row.source }}</td><td>{{ row.target }}</td><td>{{ row.result }}</td></tr></tbody></table></div>
      <p v-else-if="previewError" class="preview-error">{{ previewError }}</p>
      <p v-else class="preview-empty">{{ source ? '点击「预览传输」查看变更内容；目标未安装或版本不同，会随源复制并切换 Engine。' : '暂无可用的源实例，请先创建另一个实例。' }}</p>
    </section>
    <footer><span>{{ preview ? '预览已生成，可以执行传输' : '先预览校验数据，再执行传输' }}</span><div><a-button :type="preview ? 'secondary' : 'primary'" :loading="loading" :disabled="disabled || loading || !source" @click="showPreview">预览传输</a-button><a-button type="primary" :disabled="disabled || loading || !preview" @click="transfer">执行传输</a-button></div></footer>
    <p v-if="backup" class="backup">目标备份：{{ backup }}</p>
  </section>
</template>
<style scoped>
.transfer-card { display: grid; gap: 22px; margin-top: 0; padding: 24px; border: 1px solid #e4e7ec; border-radius: 12px; background: #fff; color: #1d2939; }
h2, h3 { margin: 0; font-size: 16px; } p { margin: 6px 0 0; color: #667085; font-size: 13px; line-height: 1.65; }
.transfer-heading { display: flex; align-items: center; justify-content: space-between; gap: 16px; }
.isolation-badge { flex-shrink: 0; padding: 4px 8px; border-radius: 5px; background: #f2f4f7; color: #667085; font-size: 12px; }
.transfer-fields { display: grid; grid-template-columns: minmax(0, 1fr) 24px minmax(0, 1fr); gap: 16px; align-items: center; }
.transfer-field { position: relative; display: grid; min-width: 0; gap: 10px; padding: 18px 16px 18px 76px; border: 1px solid #e4e7ec; border-radius: 8px; background: #fafbfc; }
.transfer-field-icon { position: absolute; top: 24px; left: 18px; display: grid; place-items: center; width: 40px; height: 40px; border-radius: 50%; color: #2563eb; background: #edf3ff; font-size: 22px; }
.transfer-field > label { display: flex; flex-wrap: wrap; align-items: baseline; gap: 8px; font-size: 13px; font-weight: 600; }
.transfer-field > label > span { color: #98a2b3; font-size: 12px; font-weight: 400; }
.transfer-direction { color: #98a2b3; text-align: center; font-size: 24px; }
.transfer-modes { min-width: 0; margin: 0; padding: 0; border: 0; }
.transfer-modes legend { margin-bottom: 12px; padding: 0; font-size: 14px; font-weight: 600; }
.mode-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }
.mode-option { display: flex; align-items: flex-start; gap: 10px; min-width: 0; padding: 16px; border: 1px solid #e4e7ec; border-radius: 8px; cursor: pointer; }
.mode-option.selected { border-color: #93b4f8; background: #f5f8ff; }
.mode-option input { flex: 0 0 auto; width: 16px; height: 16px; margin: 2px 0 0; accent-color: #2563eb; }
.mode-option span { display: grid; gap: 6px; min-width: 0; }
.mode-option strong { font-size: 14px; line-height: 20px; font-weight: 600; }
.mode-option small { color: #667085; font-size: 12px; line-height: 1.6; }
.transfer-modes:disabled .mode-option { opacity: .55; cursor: not-allowed; }
.transfer-history { display: flex; flex-wrap: wrap; align-items: center; gap: 8px 16px; }
.transfer-history > span { color: #98a2b3; font-size: 12px; }
.transfer-history :deep(.arco-checkbox) { display: inline-flex; align-items: center; }
.transfer-note { padding: 12px 16px; border-radius: 8px; background: #f8f9fb; }
.transfer-note strong { color: #475467; font-size: 12px; font-weight: 600; }
.transfer-note p { font-size: 12px; margin-top: 4px; }
.transfer-preview h3 { margin-bottom: 12px; font-size: 14px; }
.preview-empty { margin: 0; padding: 20px 16px; border: 1px dashed #d0d5dd; border-radius: 8px; color: #98a2b3; }
.preview-error { margin: 0; padding: 14px 16px; border: 1px solid #f2c5bc; border-radius: 8px; color: #a44738; background: #fff7f4; white-space: pre-wrap; overflow-wrap: anywhere; }
.preview-table-scroll { overflow-x: auto; border: 1px solid #e4e7ec; border-radius: 8px; }
table { width: 100%; border-collapse: collapse; font-size: 13px; font-variant-numeric: tabular-nums; }
th, td { padding: 12px 16px; border-bottom: 1px solid #eef0f3; text-align: right; white-space: nowrap; }
th { color: #667085; background: #f8f9fb; font-size: 12px; font-weight: 500; }
th:first-child, td:first-child { text-align: left; } tbody tr:last-child td { border-bottom: 0; }
footer { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 12px; padding-top: 18px; border-top: 1px solid #eef0f3; }
footer > span { font-size: 12px; color: #98a2b3; } footer > div { display: flex; gap: 10px; margin-left: auto; }
.backup { overflow-wrap: anywhere; }
.transfer-card :deep(.arco-select-view), .transfer-card :deep(.arco-input-wrapper) { background: #fff; border: 1px solid #d0d5dd; border-radius: 6px; }
.transfer-card :deep(.arco-btn) { height: 36px; border-radius: 6px; }
.transfer-card { background: var(--surface-strong, #fff); border-color: var(--line, #e4e7ec); color: var(--text-main, #1d2939); box-shadow: 0 10px 24px rgba(30, 53, 84, .04); }
.mode-option.selected { border-color: var(--line-strong, #93b4f8); background: #f0f6ff; }
@container (max-width: 640px) { .transfer-card { padding: 16px; gap: 18px; } .transfer-fields, .mode-grid { grid-template-columns: minmax(0, 1fr); } .transfer-direction { transform: rotate(90deg); font-size: 20px; line-height: 12px; } .isolation-badge { display: none; } }
</style>
