<script setup lang="ts">
import { t } from "../i18n";
import type { CodexAccount } from "../types/codex";

defineProps<{
  visible: boolean;
  account: CodexAccount | null;
  phoneForm: { phone: string };
  saving: boolean;
  displayName: (account: CodexAccount) => string;
}>();

defineEmits<{
  (event: "update:visible", value: boolean): void;
  (event: "save"): void;
}>();
</script>

<template>
  <a-modal
    :visible="visible"
    :title="t('绑定手机')"
    :footer="false"
    width="560px"
    @update:visible="$emit('update:visible', $event)"
  >
    <div class="modal-form">
      <a-typography-paragraph v-if="account">
        {{ t("给") }} {{ displayName(account) }} {{ t("保存一个绑定手机号，后续会直接显示在账号卡片上。") }}
      </a-typography-paragraph>
      <a-form :model="phoneForm" layout="vertical">
        <a-form-item :label="t('手机号')">
          <a-input v-model="phoneForm.phone" placeholder="+1 (724) 806-2018" />
        </a-form-item>
      </a-form>
      <div class="form-actions">
        <a-button @click="$emit('update:visible', false)">{{ t("取消") }}</a-button>
        <a-button type="primary" :loading="saving" @click="$emit('save')">
          <template #icon><icon-save /></template>
          {{ t("保存") }}
        </a-button>
      </div>
    </div>
  </a-modal>
</template>
