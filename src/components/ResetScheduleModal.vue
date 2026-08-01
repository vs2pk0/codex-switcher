<script setup lang="ts">
import { ref, watch } from "vue";
import { Message } from "@arco-design/web-vue";
import { t } from "../i18n";
import {
  formatLocalScheduleInput,
  parseLocalScheduleInput,
} from "../services/resetScheduleEntry";

const props = defineProps<{
  visible: boolean;
  accountLabel: string;
  saving: boolean;
  mode: "create" | "edit";
  initialScheduledAt?: number;
}>();

const emit = defineEmits<{
  (event: "update:visible", value: boolean): void;
  (event: "save", scheduledAt: number): void;
}>();

const scheduledAtInput = ref("");

watch(
  () => props.visible,
  (visible) => {
    if (!visible) return;
    scheduledAtInput.value = props.mode === "edit"
      ? formatLocalScheduleInput(props.initialScheduledAt ?? Number.NaN)
      : "";
  },
);

function updateVisible(visible: boolean): void {
  if (props.saving) return;
  emit("update:visible", visible);
}

function submitSchedule(): void {
  if (props.saving) return;
  const scheduledAt = parseLocalScheduleInput(scheduledAtInput.value);
  if (scheduledAt === null) {
    Message.warning(t("请选择有效的预约时间"));
    return;
  }
  if (scheduledAt <= Date.now()) {
    Message.warning(t("预约时间必须晚于当前时间"));
    return;
  }
  emit("save", scheduledAt);
}
</script>

<template>
  <a-modal
    :visible="visible"
    :title="t(mode === 'edit' ? '修改预约时间' : '设置预约重置时间')"
    :footer="false"
    :closable="!saving"
    :mask-closable="!saving"
    width="460px"
    modal-class="reset-schedule-modal"
    @update:visible="updateVisible"
  >
    <div class="reset-schedule-modal-body">
      <p>
        {{ t(mode === "edit" ? "修改以下账号的预约时间" : "将为以下账号预约一次重置") }}
      </p>
      <strong class="reset-schedule-account">{{ accountLabel }}</strong>
      <a-date-picker
        v-model="scheduledAtInput"
        show-time
        format="YYYY-MM-DD HH:mm"
        value-format="YYYY-MM-DD HH:mm"
        :placeholder="t('选择重置时间')"
        :disabled="saving"
        class="reset-schedule-date-picker"
      />
      <div class="reset-schedule-modal-actions">
        <a-button :disabled="saving" @click="updateVisible(false)">
          {{ t("取消") }}
        </a-button>
        <a-button
          type="primary"
          :loading="saving"
          :disabled="!scheduledAtInput"
          @click="submitSchedule"
        >
          <template #icon><icon-calendar /></template>
          {{ t(mode === "edit" ? "保存修改" : "保存预约") }}
        </a-button>
      </div>
    </div>
  </a-modal>
</template>
