<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { Message, Modal } from "@arco-design/web-vue";
import DOMPurify from "dompurify";
import { marked, Renderer } from "marked";
import { t } from "../i18n";
import {
  deleteSessionMessages,
  deleteSessionTurn,
  getSessionAsset,
  listSessionContent,
  openPathInFileManager,
  restoreSessionTurnBackup,
  type CodexSessionAttachment,
  type CodexSessionMessage,
  type CodexSessionRecord,
  type CodexSessionTurn,
} from "../services/session";

const props = defineProps<{
  visible: boolean;
  session: CodexSessionRecord | null;
}>();

const emit = defineEmits<{
  (event: "update:visible", value: boolean): void;
  (event: "session-updated"): void;
}>();

const turns = ref<CodexSessionTurn[]>([]);
const nextCursor = ref<number | null>(null);
const loading = ref(false);
const loadingMore = ref(false);
const deletingTurnIds = ref<Set<string>>(new Set());
const deletingMessageIds = ref<Set<string>>(new Set());
const restoring = ref(false);
const searchQuery = ref("");
const sortDirection = ref<"asc" | "desc">("desc");
const renderMarkdownEnabled = ref(true);
const assetUrls = ref<Record<string, string>>({});
const assetLoadingIds = ref<Set<string>>(new Set());
const lastBackupId = ref("");
const skillDetail = ref<SessionSkillDetail | null>(null);
const markdownCache = new Map<string, { source: string; assetKey: string; html: string }>();
let requestSequence = 0;

interface SessionSkillDetail {
  name: string;
  label: string;
  request: string;
  payload: CodexSessionMessage;
}

interface SessionMessageEntry {
  id: string;
  message: CodexSessionMessage;
  messageIds: string[];
  skill: SessionSkillDetail | null;
}

const visibleTurns = computed(() => {
  const keyword = searchQuery.value.trim().toLocaleLowerCase();
  const entries = turns.value.map((turn, index) => ({ turn, sequence: index + 1 }));
  const filtered = keyword
    ? entries.filter(({ turn }) =>
        turn.messages.some((message) =>
          [
            message.text,
            ...message.attachments.flatMap((attachment) => [attachment.name, attachment.sourcePath || ""]),
          ].some((value) => value.toLocaleLowerCase().includes(keyword)),
        ),
      )
    : entries;
  return filtered;
});

const attachmentCount = computed(() =>
  turns.value.reduce(
    (total, turn) => total + turn.messages.reduce((sum, message) => sum + message.attachments.length, 0),
    0,
  ),
);

watch(
  () => [props.visible, props.session?.id] as const,
  ([visible]) => {
    requestSequence += 1;
    resetState();
    if (visible && props.session) void loadFirstPage();
  },
);

function resetState(): void {
  turns.value = [];
  nextCursor.value = null;
  loading.value = false;
  loadingMore.value = false;
  deletingTurnIds.value = new Set();
  deletingMessageIds.value = new Set();
  restoring.value = false;
  searchQuery.value = "";
  sortDirection.value = "desc";
  renderMarkdownEnabled.value = true;
  assetUrls.value = {};
  assetLoadingIds.value = new Set();
  lastBackupId.value = "";
  skillDetail.value = null;
  markdownCache.clear();
}

function skillDisplayName(name: string): string {
  if (name.toLocaleLowerCase() === "imagegen") return "Image Gen";
  return name
    .split(/[-_\s]+/)
    .filter(Boolean)
    .map((part) => `${part.charAt(0).toLocaleUpperCase()}${part.slice(1)}`)
    .join(" ");
}

function parseSkillInvocation(text: string): { name: string; request: string } | null {
  const markdown = text.match(
    /^\s*\[\$?([A-Za-z][\w-]*)\]\([^\n)]*SKILL\.md[^\n)]*\)\s*([\s\S]*)$/i,
  );
  if (markdown) return { name: markdown[1], request: markdown[2].trim() };
  const plain = text.match(/^\s*\$([A-Za-z][\w-]*)\s*([\s\S]*)$/);
  return plain ? { name: plain[1], request: plain[2].trim() } : null;
}

function parseSkillPayloadName(text: string): string | null {
  const xmlName = text.match(/<name>\s*([^<\n]+?)\s*<\/name>/i)?.[1]?.trim();
  if (xmlName && /<skill\b|<path>[^<\n]*SKILL\.md\s*<\/path>/i.test(text)) return xmlName;
  const frontmatterName = text.match(/(?:^|\n)name:\s*["']?([A-Za-z][\w-]*)["']?\s*(?:\n|$)/i)?.[1];
  if (!frontmatterName) return null;
  return /<skill\b|SKILL\.md|(?:^|\n)#{1,3}\s+.+Skill\s*$/im.test(text)
    ? frontmatterName
    : null;
}

function messageEntries(turn: CodexSessionTurn): SessionMessageEntry[] {
  const entries: SessionMessageEntry[] = [];
  for (let index = 0; index < turn.messages.length; index += 1) {
    const message = turn.messages[index];
    const invocation = parseSkillInvocation(message.text);
    const nextMessage = turn.messages[index + 1];
    const nextSkillName = nextMessage ? parseSkillPayloadName(nextMessage.text) : null;
    if (
      invocation &&
      nextMessage?.role === "user" &&
      nextSkillName?.toLocaleLowerCase() === invocation.name.toLocaleLowerCase()
    ) {
      entries.push({
        id: `${message.id}:skill:${nextMessage.id}`,
        message,
        messageIds: [message.id, nextMessage.id],
        skill: {
          name: invocation.name,
          label: skillDisplayName(invocation.name),
          request: invocation.request,
          payload: nextMessage,
        },
      });
      index += 1;
      continue;
    }
    const payloadName = parseSkillPayloadName(message.text);
    if (payloadName) {
      entries.push({
        id: `${message.id}:skill-payload`,
        message,
        messageIds: [message.id],
        skill: {
          name: payloadName,
          label: skillDisplayName(payloadName),
          request: t("查看技能说明"),
          payload: message,
        },
      });
      continue;
    }
    entries.push({ id: message.id, message, messageIds: [message.id], skill: null });
  }
  return entries;
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

function decodeMarkdownPath(value: string): string {
  try {
    return decodeURIComponent(value).replace(/^file:\/\//, "");
  } catch {
    return value.replace(/^file:\/\//, "");
  }
}

function renderMessageMarkdown(message: CodexSessionMessage): string {
  const assetKey = message.attachments
    .filter((attachment) => Boolean(assetUrls.value[attachment.id]))
    .map((attachment) => attachment.id)
    .join("|");
  const cached = markdownCache.get(message.id);
  if (cached?.source === message.text && cached.assetKey === assetKey) return cached.html;

  const renderer = new Renderer();
  renderer.image = ({ href, text }) => {
    const normalizedHref = decodeMarkdownPath(href);
    const attachment = message.attachments.find(
      (candidate) => candidate.sourcePath === normalizedHref || candidate.sourcePath === href || candidate.name === normalizedHref,
    );
    const source = attachment ? assetUrls.value[attachment.id] : "";
    if (source) {
      return `<img src="${escapeHtml(source)}" alt="${escapeHtml(text)}" loading="lazy">`;
    }
    return `<span class="session-markdown-image-reference">${escapeHtml(text || t("图片附件"))}</span>`;
  };
  renderer.link = function ({ href, title, tokens }) {
    const label = this.parser.parseInline(tokens);
    const titleAttribute = title ? ` title="${escapeHtml(title)}"` : "";
    return `<a href="${escapeHtml(href)}" target="_blank" rel="noopener noreferrer"${titleAttribute}>${label}</a>`;
  };

  const parsed = marked.parse(message.text, { async: false, breaks: true, gfm: true, renderer });
  const html = DOMPurify.sanitize(parsed, {
    ADD_ATTR: ["target", "rel", "loading"],
    FORBID_ATTR: ["style", "srcset"],
    FORBID_TAGS: ["style"],
  });
  markdownCache.set(message.id, { source: message.text, assetKey, html });
  return html;
}

function handleMarkdownClick(event: MouseEvent): void {
  const element = event.target instanceof Element ? event.target.closest("a") : null;
  const href = element?.getAttribute("href")?.trim() || "";
  if (!href || (!href.startsWith("/") && !href.startsWith("file://") && !/^[A-Za-z]:[\\/]/.test(href))) return;
  event.preventDefault();
  void openPathInFileManager(decodeMarkdownPath(href)).catch((error) => {
    Message.error(`${t("打开附件失败")}：${String(error)}`);
  });
}

function updateVisible(visible: boolean): void {
  if (!deletingTurnIds.value.size && !deletingMessageIds.value.size && !restoring.value) {
    emit("update:visible", visible);
  }
}

function openSkillDetail(detail: SessionSkillDetail): void {
  skillDetail.value = detail;
}

function updateSkillDetailVisible(visible: boolean): void {
  if (!visible) skillDetail.value = null;
}

async function loadFirstPage(): Promise<void> {
  const session = props.session;
  if (!session) return;
  const sequence = ++requestSequence;
  loading.value = true;
  try {
    const page = await listSessionContent(session.id, null, 20, sortDirection.value);
    if (sequence !== requestSequence || props.session?.id !== session.id) return;
    turns.value = page.turns;
    nextCursor.value = page.nextCursor ?? null;
  } catch (error) {
    if (sequence === requestSequence) Message.error(`${t("读取会话内容失败")}：${String(error)}`);
  } finally {
    if (sequence === requestSequence) loading.value = false;
  }
}

async function loadMore(): Promise<void> {
  const session = props.session;
  const cursor = nextCursor.value;
  if (!session || cursor === null || loadingMore.value) return;
  const sequence = requestSequence;
  loadingMore.value = true;
  try {
    const page = await listSessionContent(session.id, cursor, 20, sortDirection.value);
    if (sequence !== requestSequence || props.session?.id !== session.id) return;
    const knownIds = new Set(turns.value.map((turn) => turn.id));
    turns.value = [...turns.value, ...page.turns.filter((turn) => !knownIds.has(turn.id))];
    nextCursor.value = page.nextCursor ?? null;
  } catch (error) {
    if (sequence === requestSequence) Message.error(`${t("读取更多会话内容失败")}：${String(error)}`);
  } finally {
    if (sequence === requestSequence) loadingMore.value = false;
  }
}

async function changeSortDirection(value: string | number | boolean): Promise<void> {
  const direction = value === "desc" ? "desc" : "asc";
  if (direction === sortDirection.value) return;
  sortDirection.value = direction;
  turns.value = [];
  nextCursor.value = null;
  await loadFirstPage();
}

function setInProgress(target: typeof deletingTurnIds, id: string, active: boolean): void {
  const next = new Set(target.value);
  if (active) next.add(id);
  else next.delete(id);
  target.value = next;
}

async function loadAsset(attachment: CodexSessionAttachment): Promise<void> {
  const session = props.session;
  if (!session || assetUrls.value[attachment.id] || assetLoadingIds.value.has(attachment.id)) return;
  const sequence = requestSequence;
  setInProgress(assetLoadingIds, attachment.id, true);
  try {
    const asset = await getSessionAsset(session.id, attachment.id);
    if (sequence !== requestSequence || props.session?.id !== session.id) return;
    assetUrls.value = { ...assetUrls.value, [attachment.id]: asset.dataUrl };
  } catch (error) {
    if (sequence === requestSequence) Message.error(`${t("加载附件失败")}：${String(error)}`);
  } finally {
    if (sequence === requestSequence) setInProgress(assetLoadingIds, attachment.id, false);
  }
}

async function openAttachment(attachment: CodexSessionAttachment): Promise<void> {
  if (!attachment.sourcePath) return;
  try {
    await openPathInFileManager(attachment.sourcePath);
  } catch (error) {
    Message.error(`${t("打开附件失败")}：${String(error)}`);
  }
}

function confirmDelete(turn: CodexSessionTurn): void {
  const session = props.session;
  if (!session || deletingTurnIds.value.has(turn.id)) return;
  Modal.confirm({
    title: t("删除这轮对话"),
    content: t("将删除这一轮中的用户消息、回复和附件引用。操作前会自动备份会话，可在本窗口撤销。"),
    okText: t("确认删除"),
    cancelText: t("取消"),
    okButtonProps: { status: "danger" },
    async onOk() {
      setInProgress(deletingTurnIds, turn.id, true);
      try {
        const result = await deleteSessionTurn(session.id, turn.id);
        lastBackupId.value = result.backupId;
        Message.success(`${t("已删除这轮对话")}，${t("已释放空间")} ${formatFileSize(result.removedBytes)}`);
        if (result.warnings.length) Message.warning(result.warnings.join("；"));
        emit("session-updated");
        await loadFirstPage();
      } catch (error) {
        Message.error(`${t("删除会话内容失败")}：${String(error)}`);
      } finally {
        setInProgress(deletingTurnIds, turn.id, false);
      }
    },
  });
}

async function writeClipboardText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    return;
  } catch {
    const textarea = document.createElement("textarea");
    textarea.value = text;
    textarea.setAttribute("readonly", "");
    textarea.style.position = "fixed";
    textarea.style.opacity = "0";
    document.body.appendChild(textarea);
    textarea.select();
    const copied = document.execCommand("copy");
    textarea.remove();
    if (!copied) throw new Error(t("无法访问系统剪贴板"));
  }
}

async function copyMessage(entry: SessionMessageEntry): Promise<void> {
  try {
    await writeClipboardText(entry.message.text);
    Message.success(t("消息已复制"));
  } catch (error) {
    Message.error(`${t("复制消息失败")}：${String(error)}`);
  }
}

function confirmDeleteMessages(turn: CodexSessionTurn, entry: SessionMessageEntry): void {
  const session = props.session;
  if (!session || deletingMessageIds.value.has(entry.id)) return;
  const groupedSkill = entry.messageIds.length > 1;
  Modal.confirm({
    title: groupedSkill ? t("删除技能调用") : t("删除这条消息"),
    content: groupedSkill
      ? t("将删除这次技能调用及其折叠的技能说明。操作前会自动备份会话，可在本窗口撤销。")
      : t("将只删除这条消息及其重复历史记录。操作前会自动备份会话，可在本窗口撤销。"),
    okText: t("确认删除"),
    cancelText: t("取消"),
    okButtonProps: { status: "danger" },
    async onOk() {
      setInProgress(deletingMessageIds, entry.id, true);
      try {
        const result = await deleteSessionMessages(session.id, turn.id, entry.messageIds);
        lastBackupId.value = result.backupId;
        if (skillDetail.value && entry.messageIds.includes(skillDetail.value.payload.id)) {
          skillDetail.value = null;
        }
        Message.success(`${t("消息已删除")}，${t("已释放空间")} ${formatFileSize(result.removedBytes)}`);
        if (result.warnings.length) Message.warning(result.warnings.join("；"));
        emit("session-updated");
        await loadFirstPage();
      } catch (error) {
        Message.error(`${t("删除消息失败")}：${String(error)}`);
      } finally {
        setInProgress(deletingMessageIds, entry.id, false);
      }
    },
  });
}

function confirmRestore(): void {
  const session = props.session;
  const backupId = lastBackupId.value;
  if (!session || !backupId || restoring.value) return;
  Modal.confirm({
    title: t("撤销上次删除"),
    content: t("将从自动备份恢复删除前的完整会话内容，当前文件也会先创建回滚备份。"),
    okText: t("确认恢复"),
    cancelText: t("取消"),
    async onOk() {
      restoring.value = true;
      try {
        const result = await restoreSessionTurnBackup(session.id, backupId);
        lastBackupId.value = "";
        Message.success(t("会话内容已恢复"));
        if (result.warnings.length) Message.warning(result.warnings.join("；"));
        emit("session-updated");
        await loadFirstPage();
      } catch (error) {
        Message.error(`${t("恢复会话内容失败")}：${String(error)}`);
      } finally {
        restoring.value = false;
      }
    },
  });
}

function formatFileSize(bytes?: number | null): string {
  const safeBytes = typeof bytes === "number" && Number.isFinite(bytes) ? bytes : 0;
  if (safeBytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = safeBytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${value.toFixed(value >= 10 || unitIndex === 0 ? 0 : 1)} ${units[unitIndex]}`;
}

function formatTime(value?: string | number | null): string {
  if (!value) return "--";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return String(value);
  return new Intl.DateTimeFormat(undefined, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(date);
}
</script>

<template>
  <a-modal
    :visible="visible"
    :footer="false"
    :closable="!deletingTurnIds.size && !deletingMessageIds.size && !restoring"
    :mask-closable="false"
    width="min(1180px, calc(100vw - 32px))"
    modal-class="session-content-modal"
    unmount-on-close
    @update:visible="updateVisible"
  >
    <template #title>{{ t("会话内容") }}</template>
    <div class="session-content-shell">
      <header class="session-content-summary">
        <div>
          <strong>{{ session?.title || t("未命名会话") }}</strong>
          <span :title="session?.projectPath">{{ session?.projectPath || t("未归属项目") }}</span>
        </div>
        <div class="session-content-summary-stats">
          <span>{{ t("已加载") }} {{ turns.length }} {{ t("轮对话") }}</span>
          <span>{{ attachmentCount }} {{ t("个附件") }}</span>
          <span>{{ formatFileSize(session?.sizeBytes) }}</span>
        </div>
      </header>

      <div class="session-content-toolbar">
        <a-input v-model="searchQuery" allow-clear :placeholder="t('搜索已加载的对话内容')">
          <template #prefix><icon-search /></template>
        </a-input>
        <a-tooltip :content="t('切换所有消息的 Markdown 预览和原始文本')">
          <div class="session-content-markdown-toggle">
            <span>{{ renderMarkdownEnabled ? t("Markdown 预览") : t("原始文本") }}</span>
            <a-switch v-model="renderMarkdownEnabled" size="small" />
          </div>
        </a-tooltip>
        <a-radio-group
          :model-value="sortDirection"
          type="button"
          class="session-content-sort"
          :disabled="loading || loadingMore"
          @change="changeSortDirection"
        >
          <a-radio value="desc">{{ t("倒序") }}</a-radio>
          <a-radio value="asc">{{ t("正序") }}</a-radio>
        </a-radio-group>
        <a-button v-if="lastBackupId" status="warning" :loading="restoring" @click="confirmRestore">
          <template #icon><icon-undo /></template>{{ t("撤销上次删除") }}
        </a-button>
      </div>

      <a-alert type="warning" class="session-content-safety-note">
        {{ t("支持删除单条消息或完整对话轮次；会话仍在生成时将禁止删除。所有删除都会先自动备份。") }}
      </a-alert>

      <a-spin :loading="loading" dot class="session-content-spin">
        <main class="session-content-list">
          <article v-for="{ turn, sequence } in visibleTurns" :key="turn.id" class="session-turn-card">
            <header class="session-turn-header">
              <div>
                <strong>{{ t("对话轮次") }} {{ sequence }}</strong>
                <span>{{ formatTime(turn.timestamp) }}</span>
                <small v-if="turn.technicalItemCount">{{ turn.technicalItemCount }} {{ t("条技术记录已折叠") }}</small>
              </div>
              <a-tooltip :content="turn.canDelete ? t('删除这轮对话') : t('当前对话尚未结束，不能删除')">
                <a-button
                  size="small"
                  status="danger"
                  :loading="deletingTurnIds.has(turn.id)"
                  :disabled="!turn.canDelete || deletingTurnIds.size > 0 || deletingMessageIds.size > 0 || restoring"
                  @click="confirmDelete(turn)"
                >
                  <template #icon><icon-delete /></template>{{ t("删除本轮") }}
                </a-button>
              </a-tooltip>
            </header>

            <section class="session-turn-messages">
              <article
                v-for="entry in messageEntries(turn)"
                :key="entry.id"
                class="session-message"
                :class="[`is-${entry.message.role}`, { 'is-skill-call': entry.skill }]"
              >
                <div class="session-message-meta">
                  <div>
                    <strong>{{ t(entry.message.role === "user" ? "用户" : "助手") }}</strong>
                    <span v-if="entry.message.phase">{{ entry.message.phase }}</span>
                    <span>{{ formatTime(entry.message.timestamp) }}</span>
                  </div>
                  <div class="session-message-actions">
                    <a-tooltip :content="t('复制消息')">
                      <a-button size="mini" type="text" :aria-label="t('复制消息')" @click="copyMessage(entry)">
                        <template #icon><icon-copy /></template>
                      </a-button>
                    </a-tooltip>
                    <a-tooltip :content="turn.canDelete ? t('删除这条消息') : t('当前对话尚未结束，不能删除')">
                      <a-button
                        size="mini"
                        type="text"
                        status="danger"
                        :aria-label="t('删除这条消息')"
                        :loading="deletingMessageIds.has(entry.id)"
                        :disabled="!turn.canDelete || deletingTurnIds.size > 0 || deletingMessageIds.size > 0 || restoring"
                        @click="confirmDeleteMessages(turn, entry)"
                      >
                        <template #icon><icon-delete /></template>
                      </a-button>
                    </a-tooltip>
                  </div>
                </div>
                <button
                  v-if="entry.skill"
                  type="button"
                  class="session-skill-call"
                  @click="openSkillDetail(entry.skill)"
                >
                  <span class="session-skill-call-icon"><icon-image v-if="entry.skill.name.toLowerCase() === 'imagegen'" /><icon-code v-else /></span>
                  <strong>{{ entry.skill.label }}</strong>
                  <span>{{ entry.skill.request || t("查看技能说明") }}</span>
                  <icon-right />
                </button>
                <div
                  v-else-if="entry.message.text && renderMarkdownEnabled"
                  class="session-message-markdown"
                  v-html="renderMessageMarkdown(entry.message)"
                  @click="handleMarkdownClick"
                ></div>
                <pre v-else-if="entry.message.text" class="session-message-text">{{ entry.message.text }}</pre>
                <div v-if="entry.message.attachments.length" class="session-attachment-grid">
                  <article
                    v-for="attachment in entry.message.attachments"
                    :key="attachment.id"
                    class="session-attachment-card"
                    :class="`is-${attachment.kind}`"
                  >
                    <a-image
                      v-if="attachment.kind === 'image' && assetUrls[attachment.id]"
                      :src="assetUrls[attachment.id]"
                      :title="attachment.name"
                      fit="contain"
                    />
                    <button
                      v-else-if="attachment.kind === 'image' && attachment.inline"
                      type="button"
                      class="session-image-placeholder"
                      :disabled="assetLoadingIds.has(attachment.id)"
                      @click="loadAsset(attachment)"
                    >
                      <icon-image />
                      <span>{{ assetLoadingIds.has(attachment.id) ? t("正在加载图片") : t("加载图片预览") }}</span>
                    </button>
                    <div v-else class="session-file-icon"><icon-file-image v-if="attachment.kind === 'image'" /><icon-file v-else /></div>
                    <div class="session-attachment-info">
                      <strong :title="attachment.name">{{ attachment.name }}</strong>
                      <span>{{ attachment.mimeType || t("文件") }} · {{ formatFileSize(attachment.sizeBytes) }}</span>
                      <small v-if="attachment.sourcePath" :title="attachment.sourcePath">{{ attachment.sourcePath }}</small>
                    </div>
                    <a-button
                      v-if="attachment.sourcePath"
                      size="mini"
                      type="text"
                      :disabled="!attachment.available"
                      @click="openAttachment(attachment)"
                    >
                      <template #icon><icon-folder /></template>{{ attachment.available ? t("在文件夹中显示") : t("源文件不可用") }}
                    </a-button>
                  </article>
                </div>
              </article>
            </section>
          </article>

          <div v-if="!loading && !visibleTurns.length" class="session-content-empty">
            <icon-message />
            <strong>{{ searchQuery ? t("没有匹配的已加载内容") : t("此会话暂无可显示的消息") }}</strong>
          </div>

          <a-button
            v-if="nextCursor !== null && !searchQuery"
            long
            :loading="loadingMore"
            class="session-content-load-more"
            @click="loadMore"
          >
            {{ sortDirection === "desc" ? t("加载更早对话") : t("加载更多对话") }}
          </a-button>
        </main>
      </a-spin>
    </div>
  </a-modal>

  <a-modal
    :visible="Boolean(skillDetail)"
    :footer="false"
    width="min(920px, calc(100vw - 32px))"
    modal-class="session-skill-detail-modal"
    unmount-on-close
    @update:visible="updateSkillDetailVisible"
  >
    <template #title>{{ skillDetail?.label || t("技能说明") }}</template>
    <div v-if="skillDetail" class="session-skill-detail">
      <header>
        <span class="session-skill-call-icon"><icon-image v-if="skillDetail.name.toLowerCase() === 'imagegen'" /><icon-code v-else /></span>
        <div>
          <strong>{{ skillDetail.label }}</strong>
          <span>{{ skillDetail.request || t("技能说明") }}</span>
        </div>
      </header>
      <div
        v-if="renderMarkdownEnabled"
        class="session-message-markdown"
        v-html="renderMessageMarkdown(skillDetail.payload)"
        @click="handleMarkdownClick"
      ></div>
      <pre v-else class="session-message-text">{{ skillDetail.payload.text }}</pre>
    </div>
  </a-modal>
</template>
