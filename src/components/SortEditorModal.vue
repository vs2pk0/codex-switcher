<script setup lang="ts">
import type { CodexAccount } from "../types/codex";
import { t } from "../i18n";
import PlanBadge from "./PlanBadge.vue";

defineProps<{
  visible: boolean;
  accounts: CodexAccount[];
  currentId: string;
  saving: boolean;
  sortDraftDraggingId: string;
  sortDraftOverId: string;
  displayName: (account: CodexAccount) => string;
  isApiKeyAccount: (account: CodexAccount) => boolean;
  planLabel: (account: CodexAccount) => string;
  planClass: (account: CodexAccount) => string;
}>();

defineEmits<{
  (event: "update:visible", visible: boolean): void;
  (event: "close"): void;
  (event: "save"): void;
  (event: "pointer-start", pointerEvent: PointerEvent, account: CodexAccount): void;
  (event: "pointer-enter", account: CodexAccount): void;
  (event: "move-step", account: CodexAccount, direction: -1 | 1): void;
}>();
</script>

<template>
  <a-modal
    :visible="visible"
    :title="t('编辑账号顺序')"
    width="820px"
    :footer="false"
    @update:visible="$event ? $emit('update:visible', true) : $emit('close')"
    @cancel="$emit('close')"
  >
    <div class="sort-editor">
      <div class="sort-editor-hint">
        <span>{{ t("拖动列表项调整顺序，保存后会写入自定义顺序。") }}</span>
        <b>{{ accounts.length }} {{ t("个账号") }}</b>
      </div>
      <div class="sort-editor-list">
        <article
          v-for="(account, index) in accounts"
          :key="account.id"
          class="sort-editor-row"
          :class="{
            dragging: sortDraftDraggingId === account.id,
            over: sortDraftOverId === account.id,
          }"
          @pointerenter="$emit('pointer-enter', account)"
        >
          <button
            class="sort-editor-grip"
            type="button"
            :title="t('按住拖动排序')"
            @pointerdown.prevent="$emit('pointer-start', $event, account)"
          >
            <icon-list />
          </button>
          <span class="sort-editor-index">{{ index + 1 }}</span>
          <div class="sort-editor-main">
            <strong>{{ displayName(account) }}</strong>
            <span>{{ isApiKeyAccount(account) ? "API Key" : "OAuth" }} · {{ account.email || account.id }}</span>
          </div>
          <PlanBadge :label="planLabel(account)" :badge-class="planClass(account)" />
          <a-tag v-if="account.id === currentId" color="arcoblue">{{ t("当前") }}</a-tag>
          <div class="sort-editor-actions">
            <a-button size="mini" :disabled="index === 0" @click="$emit('move-step', account, -1)">
              <template #icon><icon-up /></template>
            </a-button>
            <a-button size="mini" :disabled="index === accounts.length - 1" @click="$emit('move-step', account, 1)">
              <template #icon><icon-down /></template>
            </a-button>
          </div>
        </article>
      </div>
      <div class="sort-editor-footer">
        <a-button @click="$emit('close')">{{ t("取消") }}</a-button>
        <a-button type="primary" :loading="saving" @click="$emit('save')">{{ t("保存排序") }}</a-button>
      </div>
    </div>
  </a-modal>
</template>
