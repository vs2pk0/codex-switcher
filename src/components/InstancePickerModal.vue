<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { CodexInstance } from "../services/instances";
import { instanceDisplayName } from "../services/instances";

const props = withDefaults(defineProps<{
  visible: boolean;
  instances: CodexInstance[];
  title?: string;
  description?: string;
  confirmText?: string;
  requireRunning?: boolean;
}>(), {
  title: "选择 Codex 实例",
  description: "本次操作只会作用于选中的实例。",
  confirmText: "确认",
  requireRunning: false,
});

const emit = defineEmits<{
  (event: "update:visible", value: boolean): void;
  (event: "confirm", instanceId: string): void;
}>();

const selectedId = ref("default");
const options = computed(() => props.requireRunning
  ? props.instances.filter((instance) => instance.running)
  : props.instances);

watch(
  () => props.visible,
  (visible) => {
    if (!visible) return;
    const current = options.value.find((instance) => instance.id === selectedId.value);
    selectedId.value = current?.id
      || options.value.find((instance) => instance.running)?.id
      || options.value[0]?.id
      || "";
  },
);

function confirm(): void {
  if (!selectedId.value) return;
  emit("confirm", selectedId.value);
}
</script>

<template>
  <a-modal
    :visible="visible"
    :title="title"
    :ok-text="confirmText"
    cancel-text="取消"
    :ok-button-props="{ disabled: !selectedId }"
    modal-class="instance-picker-modal"
    @ok="confirm"
    @cancel="$emit('update:visible', false)"
  >
    <p class="instance-picker-description">{{ description }}</p>
    <a-radio-group v-model="selectedId" direction="vertical" class="instance-picker-list">
      <a-radio v-for="instance in options" :key="instance.id" :value="instance.id">
        <div class="instance-picker-option">
          <div>
            <strong>{{ instanceDisplayName(instance) }}</strong>
            <span>{{ instance.codexHome }}</span>
          </div>
          <a-tag :color="instance.running ? 'green' : 'gray'">
            {{ instance.running ? `运行中 · PID ${instance.pid}` : "未运行" }}
          </a-tag>
        </div>
      </a-radio>
    </a-radio-group>
    <a-empty v-if="!options.length" description="没有符合条件的实例" />
  </a-modal>
</template>

<style scoped>
.instance-picker-description { margin: 0 0 14px; color: #64748b; }
.instance-picker-list { display: grid; gap: 10px; width: 100%; }
.instance-picker-list :deep(.arco-radio) { width: 100%; margin: 0; padding: 13px 14px; border: 1px solid #dbe5f3; border-radius: 12px; background: #f8fbff; }
.instance-picker-list :deep(.arco-radio-checked) { border-color: #3b82f6; background: #eff6ff; }
.instance-picker-list :deep(.arco-radio-label) { width: calc(100% - 24px); }
.instance-picker-option { display: flex; align-items: center; justify-content: space-between; gap: 16px; }
.instance-picker-option > div { display: grid; min-width: 0; gap: 4px; }
.instance-picker-option strong { font-size: 14px; color: #172033; }
.instance-picker-option span { overflow: hidden; color: #718096; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 11px; text-overflow: ellipsis; white-space: nowrap; }
</style>
