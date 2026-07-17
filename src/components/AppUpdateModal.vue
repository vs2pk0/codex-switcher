<script setup lang="ts">
import { computed } from "vue";
import { t } from "../i18n";
import type {
  AppUpdateDownloadProgress,
  AppUpdateDownloadResult,
  AppUpdateInfo,
} from "../services/appUpdate";

const props = defineProps<{
  visible: boolean;
  info: AppUpdateInfo | null;
  progress: AppUpdateDownloadProgress | null;
  result: AppUpdateDownloadResult | null;
  error: string;
  downloading: boolean;
  cancelling: boolean;
  opening: boolean;
}>();

const emit = defineEmits<{
  (event: "close"): void;
  (event: "cancel"): void;
  (event: "retry"): void;
  (event: "open-installer"): void;
  (event: "open-releases"): void;
}>();

const totalBytes = computed(() => Number(props.progress?.totalBytes || props.info?.assetSize || 0));
const downloadedBytes = computed(() =>
  Number(props.progress?.downloadedBytes || props.result?.sizeBytes || 0),
);
const percent = computed(() => {
  if (props.result || props.progress?.status === "completed") return 1;
  if (totalBytes.value > 0) {
    return Math.max(0, Math.min(0.99, downloadedBytes.value / totalBytes.value));
  }
  if (props.progress?.status === "checking") return 0.04;
  if (props.progress?.status === "starting") return 0.08;
  return props.downloading ? 0.12 : 0;
});
const progressStatus = computed(() => {
  if (props.error || props.progress?.status === "failed") return "danger";
  if (props.result || props.progress?.status === "completed") return "success";
  return "normal";
});
const title = computed(() => {
  if (props.result) return t("更新安装包下载完成");
  if (props.error) return t("更新下载失败");
  return t("正在下载应用更新");
});
const detail = computed(() => {
  if (props.result) return `${formatBytes(props.result.sizeBytes)} · ${t("等待安装")}`;
  if (props.progress?.status === "checking") return t("正在获取最新版本");
  if (props.progress?.status === "starting") return t("正在连接下载服务器");
  if (!totalBytes.value) {
    return downloadedBytes.value > 0
      ? `${formatBytes(downloadedBytes.value)} · ${t("正在下载")}`
      : t("准备中");
  }
  return `${formatBytes(downloadedBytes.value)} / ${formatBytes(totalBytes.value)}`;
});

function formatBytes(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  const unitIndex = Math.min(units.length - 1, Math.floor(Math.log(value) / Math.log(1024)));
  const amount = value / 1024 ** unitIndex;
  return `${amount >= 100 || unitIndex === 0 ? amount.toFixed(0) : amount.toFixed(1)} ${units[unitIndex]}`;
}

function close(): void {
  if (!props.downloading) emit("close");
}
</script>

<template>
  <a-modal
    :visible="visible"
    :title="title"
    width="620px"
    :footer="false"
    :closable="!downloading"
    :mask-closable="!downloading"
    :esc-to-close="!downloading"
    @cancel="close"
  >
    <div class="app-update-modal">
      <div class="app-update-version-row">
        <div>
          <span>{{ t("当前版本") }}</span>
          <strong>v{{ info?.currentVersion || "--" }}</strong>
        </div>
        <icon-right />
        <div>
          <span>{{ t("最新版本") }}</span>
          <strong>v{{ info?.latestVersion || result?.version || "--" }}</strong>
        </div>
        <a-tag>{{ info?.target || "--" }}</a-tag>
      </div>

      <div class="app-update-package">
        <icon-file />
        <div>
          <strong>{{ progress?.assetName || result?.assetName || info?.assetName || t("更新安装包") }}</strong>
          <span>{{ detail }}</span>
        </div>
      </div>

      <a-progress :percent="percent" :status="progressStatus" />

      <a-alert v-if="error" type="error" show-icon>
        {{ error }}
      </a-alert>
      <a-alert v-else-if="result" type="success" show-icon>
        {{ t("安装包已保存，请打开安装包并按提示完成更新。") }}
      </a-alert>

      <div class="app-update-actions">
        <a-button v-if="downloading" :loading="cancelling" @click="emit('cancel')">
          {{ t("取消下载") }}
        </a-button>
        <template v-else>
          <a-button @click="emit('close')">{{ t("关闭") }}</a-button>
          <a-button v-if="error" @click="emit('open-releases')">
            {{ t("打开 Releases") }}
          </a-button>
          <a-button v-if="error" type="primary" @click="emit('retry')">
            <template #icon><icon-refresh /></template>
            {{ t("重新下载") }}
          </a-button>
          <a-button v-if="result" type="primary" :loading="opening" @click="emit('open-installer')">
            <template #icon><icon-download /></template>
            {{ t("打开安装包") }}
          </a-button>
        </template>
      </div>
    </div>
  </a-modal>
</template>

<style scoped>
.app-update-modal {
  display: grid;
  gap: 18px;
}

.app-update-version-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr) auto;
  align-items: center;
  gap: 14px;
  padding: 14px 16px;
  border: 1px solid #e5eaf3;
  border-radius: 8px;
  background: #f7f9fc;
}

.app-update-version-row > div {
  display: grid;
  gap: 4px;
}

.app-update-version-row span,
.app-update-package span {
  color: #6b7a90;
  font-size: 13px;
}

.app-update-version-row strong {
  color: #172033;
  font-size: 18px;
}

.app-update-package {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  align-items: center;
  gap: 12px;
}

.app-update-package > svg {
  color: #2468f2;
  font-size: 24px;
}

.app-update-package > div {
  min-width: 0;
  display: grid;
  gap: 5px;
}

.app-update-package strong {
  overflow: hidden;
  color: #172033;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.app-update-actions {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
}

@media (max-width: 680px) {
  .app-update-version-row {
    grid-template-columns: 1fr;
  }

  .app-update-version-row > svg {
    transform: rotate(90deg);
  }
}
</style>
