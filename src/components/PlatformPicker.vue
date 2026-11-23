<script setup lang="ts">
import { computed } from "vue";
import { t } from "../i18n";

const props = defineProps<{
  choices: { key: string; label: string; iconUrl: string | null }[];
  allowAll?: boolean;
}>();
const selection = defineModel<string>({ required: true });
const active = computed(() => props.choices.find(({ key }) => key === selection.value));
const popup = { contentClass: "platform-picker-popup", autoFitPopupWidth: false };
</script>

<template>
  <a-select
    v-model="selection"
    :placeholder="t('选择平台')"
    :aria-label="t('平台')"
    :trigger-props="popup"
    :scrollbar="false"
    allow-search
  >
    <template #prefix>
      <img v-if="active?.iconUrl" :src="active.iconUrl" class="platform-picker-icon" alt="" />
      <icon-apps v-else class="platform-picker-icon" aria-hidden="true" />
    </template>
    <a-option v-if="allowAll" value="" :label="t('全部平台')">
      <span class="platform-picker-option">
        <icon-apps class="platform-picker-icon" aria-hidden="true" />
        {{ t("全部平台") }}
      </span>
    </a-option>
    <a-option v-for="choice in choices" :key="choice.key" :value="choice.key" :label="choice.label">
      <span class="platform-picker-option" :title="choice.label">
        <img v-if="choice.iconUrl" :src="choice.iconUrl" class="platform-picker-icon" alt="" />
        <icon-apps v-else class="platform-picker-icon" aria-hidden="true" />
        <span>{{ choice.label }}</span>
      </span>
    </a-option>
  </a-select>
</template>

<style scoped>
.platform-picker-option {
  display: flex;
  align-items: center;
  gap: 8px;
}
.platform-picker-option span {
  overflow: hidden;
  text-overflow: ellipsis;
}
.platform-picker-icon {
  width: 16px;
  height: 16px;
  flex: none;
  object-fit: contain;
}
:global(.platform-picker-popup) {
  width: min(560px, calc(100vw - 24px));
}
:global(.platform-picker-popup .arco-select-dropdown-list-wrapper) {
  max-height: min(430px, calc(100vh - 160px));
}
:global(.platform-picker-popup .arco-select-dropdown-list) {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(165px, 1fr));
}
</style>
