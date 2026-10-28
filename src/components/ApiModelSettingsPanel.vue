<script setup lang="ts">
import { computed } from "vue";
import { t } from "../i18n";
import { maxAutoCompactTokenLimit, reasoningEffortOptions, type ApiModelSettings } from "../services/apiModelSettings";

const props = defineProps<{ settings: ApiModelSettings; saving: boolean }>();
const emit = defineEmits<{ (event: "save"): void }>();
const marks = computed(() => Object.fromEntries(props.settings.reasoningLevels.map((level, index) => [index, level])));
const effortIndex = computed(() => Math.max(0, props.settings.reasoningLevels.indexOf(props.settings.reasoningEffort)));

function formatEffortTooltip(value: number): string {
  return props.settings.reasoningLevels[value] || "";
}

function changeLevels(value: unknown): void {
  if (!Array.isArray(value) || !value.every(level => typeof level === "string")) return;
  props.settings.reasoningLevels = value;
  if (!value.includes(props.settings.reasoningEffort)) props.settings.reasoningEffort = value[0] ?? "";
  emit("save");
}

function changeEffort(value: number | number[]): void {
  if (typeof value !== "number") return;
  const effort = props.settings.reasoningLevels[value];
  if (!effort) return;
  props.settings.reasoningEffort = effort;
  emit("save");
}
</script>

<template>
  <a-card :bordered="false" class="settings-card api-model-settings">
    <div class="api-settings-heading"><h2>{{ t("API 账号偏好") }}</h2><p>{{ t("自动保存，在下次切换或启动 API 账号时应用到目标实例。") }}</p></div>
    <div class="api-setting-row">
      <div><strong>{{ t("自定义上下文压缩阈值") }}</strong><p>{{ t("达到指定 Token 数时触发自动压缩；关闭后使用原有配置。") }}</p></div>
      <a-switch v-model="settings.autoCompactEnabled" :disabled="saving" :aria-label="t('自定义上下文压缩阈值')" @change="emit('save')" />
    </div>
    <div v-if="settings.autoCompactEnabled" class="api-setting-detail">
      <label>{{ t("自动压缩阈值") }}</label>
      <a-input-number v-model="settings.autoCompactTokenLimit" :disabled="saving" :aria-label="t('自动压缩阈值')" :precision="0" :min="1" :max="maxAutoCompactTokenLimit" :step="10000" @change="emit('save')"><template #suffix>Tokens</template></a-input-number>
    </div>
    <div class="api-setting-row">
      <div><strong>{{ t("思考强度调节") }}</strong><p>{{ t("为未声明思考能力的模型补充滑条档位，并设置默认思考强度。") }}</p></div>
      <a-switch v-model="settings.reasoningEnabled" :disabled="saving" :aria-label="t('思考强度调节')" @change="emit('save')" />
    </div>
    <div v-if="settings.reasoningEnabled" class="api-setting-detail api-reasoning-detail">
      <label>{{ t("支持的思考档位") }}</label>
      <a-select :model-value="settings.reasoningLevels" multiple allow-search allow-create :disabled="saving" :aria-label="t('支持的思考档位')" @change="changeLevels">
        <a-option v-for="effort in reasoningEffortOptions" :key="effort" :value="effort">{{ effort }}</a-option>
      </a-select>
      <p>{{ t("按从低到高的顺序选择 API 实际支持的档位；模型已声明的能力优先。") }}</p>
      <label>{{ t("默认思考强度") }} <code>{{ settings.reasoningEffort }}</code></label>
      <a-slider :model-value="effortIndex" :aria-label="t('默认思考强度')" :min="0" :max="Math.max(1, settings.reasoningLevels.length - 1)" :marks="marks" :step="1" :disabled="saving || settings.reasoningLevels.length < 2" show-ticks :format-tooltip="formatEffortTooltip" @change="changeEffort" />
    </div>
    <div class="api-setting-row">
      <div><strong>{{ t("Fast 加速模式") }}</strong><p>{{ t("开启后默认使用 Fast 档位；需 API 和模型支持，费用以服务商为准。关闭后恢复原有配置。") }}</p></div>
      <a-switch v-model="settings.fastMode" :disabled="saving" :aria-label="t('Fast 加速模式')" @change="emit('save')" />
    </div>
    <div class="api-setting-row">
      <div><strong>{{ t("禁用内置联网搜索") }}</strong><p>{{ t("API 不支持 web_search 工具时开启，避免请求因工具类型不受支持而失败。") }}</p></div>
      <a-switch v-model="settings.disableWebSearch" :disabled="saving" :aria-label="t('禁用内置联网搜索')" @change="emit('save')" />
    </div>
  </a-card>
</template>

<style scoped>
.api-model-settings { margin-top: 20px; }
.api-settings-heading h2 { margin: 0; color: #172033; font-size: 16px; }
.api-settings-heading p, .api-setting-row p, .api-setting-detail p { margin: 6px 0 0; color: #8390a4; font-size: 12px; line-height: 1.6; }
.api-setting-row { display: flex; justify-content: space-between; align-items: center; gap: 20px; padding: 18px 0; border-bottom: 1px solid #edf0f5; }
.api-setting-row:last-child { border-bottom: 0; padding-bottom: 0; }
.api-setting-row strong, .api-setting-detail label { color: #28364b; font-size: 14px; }
.api-setting-row :deep(.arco-switch) { flex-shrink: 0; }
.api-setting-detail { display: grid; gap: 12px; max-width: 640px; padding: 16px 0; }
.api-setting-detail :deep(.arco-input-number) { width: min(100%, 300px); }
.api-setting-detail p { margin: 0; }
.api-setting-detail code { margin-left: 8px; color: #3370ff; }
.api-reasoning-detail :deep(.arco-slider) { width: calc(100% - 40px); margin: 0 20px 20px; }
</style>
