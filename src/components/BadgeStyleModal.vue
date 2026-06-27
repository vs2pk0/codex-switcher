<script setup lang="ts">
import { reactive, ref, watch } from "vue";
import type { CodexSwitcherSettings } from "../services/codex";
import {
  BADGE_ACCOUNT_TYPES,
  BADGE_STYLE_OPTIONS,
  defaultBadgeStyles,
} from "../constants/badgeStyles";
import PlanBadge from "./PlanBadge.vue";

const props = defineProps<{
  visible: boolean;
  settings: CodexSwitcherSettings;
  saving: boolean;
}>();

const emit = defineEmits<{
  (event: "update:visible", value: boolean): void;
  (event: "save"): void;
}>();

const activeType = ref(BADGE_ACCOUNT_TYPES[0]?.key ?? "free");
const localStyles = reactive<Record<string, string>>(defaultBadgeStyles());

function syncLocalStyles(): void {
  const defaults = defaultBadgeStyles();
  for (const type of BADGE_ACCOUNT_TYPES) {
    localStyles[type.key] = props.settings.badgeStyles?.[type.key] || defaults[type.key];
  }
}

function close(): void {
  emit("update:visible", false);
}

function selectStyle(typeKey: string, style: string): void {
  localStyles[typeKey] = style;
}

function restoreDefault(): void {
  Object.assign(localStyles, defaultBadgeStyles());
}

function confirm(): void {
  props.settings.badgeStyles = { ...localStyles };
  emit("save");
  close();
}

watch(
  () => props.visible,
  (visible) => {
    if (visible) syncLocalStyles();
  },
  { immediate: true },
);
</script>

<template>
  <a-modal
    :visible="visible"
    title="徽章图标样式"
    width="1080px"
    :footer="false"
    modal-class="badge-style-modal"
    @cancel="close"
  >
    <div class="badge-type-tabs">
      <button
        v-for="type in BADGE_ACCOUNT_TYPES"
        :key="type.key"
        class="badge-type-tab"
        :class="{ active: activeType === type.key }"
        type="button"
        @click="activeType = type.key"
      >
        <PlanBadge :label="type.label" :badge-class="[type.planClass, `badge-${localStyles[type.key]}`]" />
        <strong>{{ type.label }}</strong>
      </button>
    </div>

    <section
      v-for="type in BADGE_ACCOUNT_TYPES"
      v-show="activeType === type.key"
      :key="type.key"
      class="badge-style-section"
    >
      <div class="badge-style-section-head">
        <h3>{{ type.label }} 徽章样式</h3>
        <span>30 套视觉方案</span>
      </div>
      <div class="badge-style-grid badge-style-grid-modal">
        <button
          v-for="style in BADGE_STYLE_OPTIONS"
          :key="style.value"
          class="badge-style-card"
          :class="{ active: localStyles[type.key] === style.value }"
          type="button"
          @click="selectStyle(type.key, style.value)"
        >
          <PlanBadge :label="type.label" :badge-class="[type.planClass, `badge-${style.value}`]" />
          <strong>{{ style.label }}</strong>
        </button>
      </div>
    </section>

    <div class="badge-modal-footer">
      <a-button @click="restoreDefault">
        <template #icon><icon-refresh /></template>
        恢复默认
      </a-button>
      <a-button type="primary" :loading="saving" @click="confirm">确认</a-button>
    </div>
  </a-modal>
</template>
