<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { Message } from "@arco-design/web-vue";
import { open } from "@tauri-apps/plugin-dialog";
import { t } from "../i18n";
import { instanceDisplayName, type CodexInstance } from "../services/instances";
import {
  listSessionsAcrossInstances,
  type CodexSessionRecord,
} from "../services/session";

interface SessionCopyDirectoryOption {
  name: string;
  path: string;
}

const props = defineProps<{
  visible: boolean;
  source: CodexSessionRecord | null;
  instances: CodexInstance[];
  sourceInstanceId: string;
  directories: SessionCopyDirectoryOption[];
  saving: boolean;
}>();

const emit = defineEmits<{
  (event: "update:visible", value: boolean): void;
  (event: "save", projectPath: string, targetInstanceId: string): void;
}>();

const projectPath = ref("");
const targetInstanceId = ref("default");
const targetDirectories = ref<SessionCopyDirectoryOption[]>([]);
const directoriesLoading = ref(false);
let directoryRequestSequence = 0;
const targetInstance = computed(() =>
  props.instances.find((instance) => instance.id === targetInstanceId.value),
);
const sourceInstance = computed(() =>
  props.instances.find((instance) => instance.id === props.sourceInstanceId),
);
const availableDirectories = computed(() =>
  targetDirectories.value.filter(
    (directory) =>
      targetInstanceId.value !== props.sourceInstanceId
      || directory.path !== props.source?.projectPath,
  ),
);

watch(
  () => [props.visible, props.source?.id, props.sourceInstanceId],
  ([visible]) => {
    if (visible) {
      targetInstanceId.value = props.sourceInstanceId || props.instances[0]?.id || "default";
      projectPath.value = "";
      void loadTargetDirectories(targetInstanceId.value);
    } else {
      directoryRequestSequence += 1;
      directoriesLoading.value = false;
    }
  },
);

watch(targetInstanceId, (instanceId) => {
  if (!props.visible) return;
  projectPath.value = "";
  void loadTargetDirectories(instanceId);
});

watch(
  () => props.directories,
  (directories) => {
    if (props.visible && targetInstanceId.value === props.sourceInstanceId) {
      targetDirectories.value = directories;
    }
  },
  { deep: true },
);

function directoriesFromSessions(sessions: CodexSessionRecord[]): SessionCopyDirectoryOption[] {
  const directories = new Map<string, SessionCopyDirectoryOption>();
  for (const session of sessions) {
    const path = session.projectPath.trim();
    if (!path || directories.has(path)) continue;
    directories.set(path, {
      name: session.projectName.trim() || path.split(/[\\/]/).filter(Boolean).at(-1) || path,
      path,
    });
  }
  return [...directories.values()];
}

async function loadTargetDirectories(instanceId: string): Promise<void> {
  const requestSequence = ++directoryRequestSequence;
  directoriesLoading.value = true;
  targetDirectories.value = [];
  try {
    const directories = instanceId === props.sourceInstanceId
      ? props.directories
      : directoriesFromSessions(await listSessionsAcrossInstances({ instanceId }));
    if (
      requestSequence !== directoryRequestSequence
      || !props.visible
      || instanceId !== targetInstanceId.value
    ) return;
    targetDirectories.value = directories;
  } catch (error) {
    if (
      requestSequence === directoryRequestSequence
      && props.visible
      && instanceId === targetInstanceId.value
    ) {
      Message.error(`${t("加载目标实例目录失败")}：${String(error)}`);
    }
  } finally {
    if (requestSequence === directoryRequestSequence) directoriesLoading.value = false;
  }
}

function updateVisible(visible: boolean): void {
  if (!props.saving) emit("update:visible", visible);
}

async function chooseDirectory(): Promise<void> {
  try {
    const selected = await open({
      multiple: false,
      directory: true,
      defaultPath: projectPath.value || props.source?.projectPath || undefined,
    });
    if (typeof selected === "string" && selected) projectPath.value = selected;
  } catch (error) {
    Message.error(`${t("选择工作目录失败")}：${String(error)}`);
  }
}

function submit(): void {
  const targetPath = projectPath.value.trim();
  if (!targetPath) {
    Message.warning(t("请选择副本要归属的工作目录"));
    return;
  }
  emit("save", targetPath, targetInstanceId.value);
}
</script>

<template>
  <a-modal
    :visible="visible"
    :title="t('创建会话副本')"
    :footer="false"
    :closable="!saving"
    :mask-closable="!saving"
    width="760px"
    modal-class="session-copy-modal"
    @update:visible="updateVisible"
  >
    <div class="session-copy-modal-body">
      <div class="session-copy-source">
        <span>{{ t("源会话") }}</span>
        <strong>{{ source?.title || t("未命名会话") }}</strong>
        <small>{{ source?.projectPath || source?.projectName }}</small>
        <small v-if="sourceInstance">{{ t("源实例") }}：{{ instanceDisplayName(sourceInstance) }}</small>
      </div>
      <a-alert type="info">
        {{ t("副本会显示在目标目录分组中，并与该目录已有会话共存；源会话及其他已有会话不会被修改。") }}
      </a-alert>
      <div v-if="instances.length > 1" class="session-copy-instance-field">
        <span>{{ t("目标实例") }}</span>
        <a-select
          v-model="targetInstanceId"
          :aria-label="t('目标实例')"
          :disabled="saving"
          popup-container=".session-copy-modal"
          :trigger-props="{ contentClass: 'session-copy-select-popup' }"
        >
          <a-option
            v-for="instance in instances"
            :key="instance.id"
            :value="instance.id"
            :label="`${instanceDisplayName(instance)} · ${instance.running ? t('运行中') : t('未运行')}`"
          >
            <div class="session-copy-option-content">
              <strong>{{ instanceDisplayName(instance) }}</strong>
              <span>{{ instance.running ? t("运行中") : t("未运行") }} · {{ instance.codexHome }}</span>
            </div>
          </a-option>
        </a-select>
        <small v-if="targetInstance" class="session-copy-selected-path">
          {{ t("副本会写入") }} {{ targetInstance.codexHome }}/sessions，{{ t("完成后将重启该实例") }}
        </small>
      </div>
      <div class="session-copy-directory-field">
        <span>{{ t("目标工作目录") }}</span>
        <div class="session-copy-directory-input">
          <a-select
            v-model="projectPath"
            :aria-label="t('目标工作目录')"
            allow-search
            allow-clear
            :disabled="saving || directoriesLoading"
            :loading="directoriesLoading"
            :placeholder="t('从已有项目选择')"
            popup-container=".session-copy-modal"
            :trigger-props="{ contentClass: 'session-copy-select-popup' }"
          >
            <a-option
              v-for="directory in availableDirectories"
              :key="directory.path"
              :value="directory.path"
              :label="directory.name"
            >
              <div class="session-copy-option-content">
                <strong class="session-copy-directory-name">{{ directory.name }}</strong>
                <span class="session-copy-project-path">{{ directory.path }}</span>
              </div>
            </a-option>
          </a-select>
          <a-button :disabled="saving" @click="chooseDirectory">
            <template #icon><icon-folder /></template>
            {{ t("选择其他目录") }}
          </a-button>
        </div>
        <small v-if="projectPath" class="session-copy-selected-path">{{ projectPath }}</small>
      </div>
      <div class="session-copy-target">
        <span>{{ t("新会话名称") }}</span>
        <strong>{{ source?.title || t("未命名会话") }} {{ t("副本") }}</strong>
      </div>
      <div class="session-mutation-actions">
        <a-button :disabled="saving" @click="updateVisible(false)">{{ t("取消") }}</a-button>
        <a-button type="primary" :loading="saving" :disabled="!projectPath.trim()" @click="submit">
          <template #icon><icon-copy /></template>
          {{ t("创建会话副本") }}
        </a-button>
      </div>
    </div>
  </a-modal>
</template>
