<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { CodexConfigFileContent, CodexConfigFileKind } from "../services/codex";
import { t } from "../i18n";

const props = defineProps<{
  visible: boolean;
  fileKind: CodexConfigFileKind;
  file: CodexConfigFileContent | null;
  content: string;
  loading: boolean;
  saving: boolean;
  formatting: boolean;
}>();

const emit = defineEmits<{
  (event: "update:visible", value: boolean): void;
  (event: "update:content", value: string): void;
  (event: "reload"): void;
  (event: "format"): void;
  (event: "save"): void;
}>();

const fileName = computed(() => props.file?.name || (props.fileKind === "auth" ? "auth.json" : "config.toml"));
const languageLabel = computed(() => (props.fileKind === "auth" ? "JSON" : "TOML"));

const editorRef = ref<unknown>(null);
const searchInputRef = ref<unknown>(null);
const highlightContentRef = ref<HTMLElement | null>(null);
const searchVisible = ref(false);
const searchQuery = ref("");
const activeMatchIndex = ref(-1);
const MAX_RENDERED_SEARCH_MATCHES = 600;
let scrollBoundTextarea: HTMLTextAreaElement | null = null;
let editorResizeObserver: ResizeObserver | null = null;

function unwrapElement(value: unknown): HTMLElement | null {
  if (value instanceof HTMLElement) return value;
  if (value && typeof value === "object" && "$el" in value) {
    const element = (value as { $el?: unknown }).$el;
    return element instanceof HTMLElement ? element : null;
  }
  return null;
}

function editorTextarea(): HTMLTextAreaElement | null {
  const element = unwrapElement(editorRef.value);
  if (element instanceof HTMLTextAreaElement) return element;
  return element?.querySelector("textarea") ?? null;
}

function searchInput(): HTMLInputElement | null {
  const element = unwrapElement(searchInputRef.value);
  if (element instanceof HTMLInputElement) return element;
  return element?.querySelector("input") ?? null;
}

const searchMatches = computed(() => {
  const query = searchQuery.value;
  if (!query) return [];

  const matcher = new RegExp(query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "giu");
  const matches: Array<{ start: number; end: number }> = [];
  let match = matcher.exec(props.content);
  while (match) {
    matches.push({ start: match.index, end: match.index + match[0].length });
    match = matcher.exec(props.content);
  }
  return matches;
});

const searchPosition = computed(() => {
  const total = searchMatches.value.length;
  const current = total && activeMatchIndex.value >= 0 ? activeMatchIndex.value + 1 : 0;
  return `${current} / ${total}`;
});

const searchHighlightVisible = computed(() => searchVisible.value && Boolean(searchQuery.value));

const searchHighlightSegments = computed(() => {
  if (!searchHighlightVisible.value) return [];

  const segments: Array<{ text: string; matchIndex: number | null }> = [];
  const total = searchMatches.value.length;
  const centerIndex = activeMatchIndex.value >= 0 ? activeMatchIndex.value : 0;
  const maxStart = Math.max(0, total - MAX_RENDERED_SEARCH_MATCHES);
  const windowStart = Math.min(
    maxStart,
    Math.max(0, centerIndex - Math.floor(MAX_RENDERED_SEARCH_MATCHES / 2)),
  );
  const windowEnd = Math.min(total, windowStart + MAX_RENDERED_SEARCH_MATCHES);
  let cursor = 0;
  searchMatches.value.slice(windowStart, windowEnd).forEach((match, offset) => {
    if (match.start > cursor) {
      segments.push({ text: props.content.slice(cursor, match.start), matchIndex: null });
    }
    segments.push({
      text: props.content.slice(match.start, match.end),
      matchIndex: windowStart + offset,
    });
    cursor = match.end;
  });
  if (cursor < props.content.length || !segments.length) {
    segments.push({ text: props.content.slice(cursor), matchIndex: null });
  }
  return segments;
});

function syncHighlightScroll() {
  const textarea = scrollBoundTextarea || editorTextarea();
  const highlight = highlightContentRef.value;
  if (!textarea || !highlight) return;
  highlight.style.width = `${textarea.clientWidth}px`;
  highlight.style.transform = `translate(0, ${-textarea.scrollTop}px)`;
}

function unbindEditorScroll() {
  scrollBoundTextarea?.removeEventListener("scroll", syncHighlightScroll);
  editorResizeObserver?.disconnect();
  editorResizeObserver = null;
  scrollBoundTextarea = null;
}

function bindEditorScroll() {
  const textarea = editorTextarea();
  if (textarea === scrollBoundTextarea) {
    syncHighlightScroll();
    return;
  }
  unbindEditorScroll();
  scrollBoundTextarea = textarea;
  scrollBoundTextarea?.addEventListener("scroll", syncHighlightScroll, { passive: true });
  if (scrollBoundTextarea) {
    editorResizeObserver = new ResizeObserver(syncHighlightScroll);
    editorResizeObserver.observe(scrollBoundTextarea);
  }
  syncHighlightScroll();
}

async function revealMatch(index: number) {
  const matches = searchMatches.value;
  const textarea = editorTextarea();
  if (!matches.length || !textarea) {
    activeMatchIndex.value = -1;
    return;
  }

  const normalizedIndex = ((index % matches.length) + matches.length) % matches.length;
  const { start: matchAt, end: matchEnd } = matches[normalizedIndex];
  activeMatchIndex.value = normalizedIndex;
  textarea.setSelectionRange(matchAt, matchEnd);

  await nextTick();
  syncHighlightScroll();
  const activeMatch = highlightContentRef.value?.querySelector<HTMLElement>(
    ".config-code-search-match.active",
  );
  if (!activeMatch) return;

  const style = window.getComputedStyle(textarea);
  const lineHeight = Number.parseFloat(style.lineHeight) || 21;
  const editorRect = textarea.getBoundingClientRect();
  const matchRect = activeMatch.getClientRects()[0] || activeMatch.getBoundingClientRect();
  const matchTop = textarea.scrollTop + matchRect.top - editorRect.top;
  const targetTop = Math.max(
    0,
    Math.min(
      textarea.scrollHeight - textarea.clientHeight,
      matchTop - textarea.clientHeight / 2 + Math.min(matchRect.height, lineHeight) / 2,
    ),
  );
  const behavior = window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth";
  textarea.scrollTo({ top: targetTop, left: 0, behavior });
  void nextTick(syncHighlightScroll);
}

function collapseMatchSelection() {
  const textarea = editorTextarea();
  if (!textarea) return;
  const caret = textarea.selectionEnd;
  textarea.setSelectionRange(caret, caret);
}

function findMatch(direction: 1 | -1) {
  if (!searchMatches.value.length) {
    searchInput()?.focus();
    return;
  }
  const currentIndex = activeMatchIndex.value < 0 ? (direction > 0 ? -1 : 0) : activeMatchIndex.value;
  void revealMatch(currentIndex + direction);
}

async function openSearch() {
  bindEditorScroll();
  const textarea = editorTextarea();
  const selectedText = textarea?.selectionStart !== textarea?.selectionEnd
    ? props.content.slice(textarea?.selectionStart || 0, textarea?.selectionEnd || 0)
    : "";
  if (selectedText && !selectedText.includes("\n") && selectedText.length <= 120) {
    searchQuery.value = selectedText;
    activeMatchIndex.value = -1;
  }

  searchVisible.value = true;
  await nextTick();
  bindEditorScroll();
  if (searchMatches.value.length) {
    await revealMatch(activeMatchIndex.value < 0 ? 0 : activeMatchIndex.value);
  }
  searchInput()?.focus();
  searchInput()?.select();
}

async function closeSearch() {
  searchVisible.value = false;
  activeMatchIndex.value = -1;
  await nextTick();
  editorTextarea()?.focus();
}

async function updateSearchQuery(value: string) {
  searchQuery.value = String(value);
  activeMatchIndex.value = -1;
  await nextTick();
  if (searchMatches.value.length) {
    await revealMatch(0);
  } else {
    collapseMatchSelection();
  }
}

function handleGlobalKeydown(event: KeyboardEvent) {
  if (!props.visible) return;
  if ((event.ctrlKey || event.metaKey) && !event.altKey && event.key.toLocaleLowerCase() === "f") {
    event.preventDefault();
    event.stopPropagation();
    void openSearch();
    return;
  }
  if (searchVisible.value && event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    void closeSearch();
  }
}

watch(
  () => props.visible,
  async (visible) => {
    if (visible) {
      await nextTick();
      bindEditorScroll();
      return;
    }
    unbindEditorScroll();
    searchVisible.value = false;
    searchQuery.value = "";
    activeMatchIndex.value = -1;
  },
);

watch(searchMatches, (matches) => {
  if (!searchVisible.value) return;
  if (!matches.length) {
    activeMatchIndex.value = -1;
  } else if (activeMatchIndex.value >= matches.length) {
    activeMatchIndex.value = matches.length - 1;
  }
});

watch(searchHighlightSegments, async () => {
  await nextTick();
  syncHighlightScroll();
});

onMounted(() => {
  document.addEventListener("keydown", handleGlobalKeydown, true);
  if (props.visible) void nextTick(bindEditorScroll);
});
onBeforeUnmount(() => {
  unbindEditorScroll();
  document.removeEventListener("keydown", handleGlobalKeydown, true);
});
</script>

<template>
  <a-modal
    :visible="visible"
    :width="960"
    :footer="false"
    :unmount-on-close="true"
    modal-class="codex-config-editor-modal"
    @cancel="$emit('update:visible', false)"
  >
    <template #title>
      <span class="config-editor-title">
        <icon-code />
        {{ t("编辑") }} {{ fileName }}
        <em>{{ languageLabel }}</em>
      </span>
    </template>

    <a-spin :loading="loading" dot>
      <div class="config-editor-body">
        <div class="config-editor-meta">
          <div>
            <strong>{{ fileName }}</strong>
            <span>{{ file?.exists ? t("文件已存在") : t("文件不存在，保存时会创建") }}</span>
          </div>
          <code :title="file?.path">{{ file?.path || "~/.codex" }}</code>
        </div>

        <a-alert v-if="fileKind === 'auth'" type="warning" class="config-editor-warning">
          {{ t("auth.json 包含登录令牌等敏感信息，请勿截图、复制或分享给他人。") }}
        </a-alert>

        <div v-if="searchVisible" class="config-editor-search">
          <icon-search />
          <a-input
            ref="searchInputRef"
            class="config-editor-search-input"
            :model-value="searchQuery"
            :placeholder="t('查找配置内容')"
            allow-clear
            @input="updateSearchQuery(String($event))"
            @clear="updateSearchQuery('')"
            @keydown.enter.prevent="findMatch($event.shiftKey ? -1 : 1)"
          />
          <span class="config-editor-search-count">{{ searchPosition }}</span>
          <a-tooltip :content="t('上一个匹配')">
            <a-button size="mini" :disabled="!searchMatches.length" :aria-label="t('上一个匹配')" @click="findMatch(-1)">
              <template #icon><icon-up /></template>
            </a-button>
          </a-tooltip>
          <a-tooltip :content="t('下一个匹配')">
            <a-button size="mini" :disabled="!searchMatches.length" :aria-label="t('下一个匹配')" @click="findMatch(1)">
              <template #icon><icon-down /></template>
            </a-button>
          </a-tooltip>
          <a-button size="mini" :aria-label="t('关闭查找')" @click="closeSearch">
            <template #icon><icon-close /></template>
          </a-button>
        </div>

        <div
          class="config-code-editor-shell"
          :class="{ 'search-highlight-active': searchHighlightVisible }"
        >
          <div v-if="searchHighlightVisible" class="config-code-highlight-layer" aria-hidden="true">
            <pre ref="highlightContentRef" class="config-code-highlight-content"><span
              v-for="(segment, index) in searchHighlightSegments"
              :key="index"
              :class="{
                'config-code-search-match': segment.matchIndex !== null,
                active: segment.matchIndex === activeMatchIndex,
              }"
            >{{ segment.text }}</span></pre>
          </div>
          <a-textarea
            ref="editorRef"
            class="config-code-editor"
            :model-value="content"
            :textarea-attrs="{ wrap: 'soft' }"
            :disabled="loading || saving"
            :placeholder="t('请输入配置内容')"
            @input="$emit('update:content', String($event))"
          />
        </div>

        <div class="config-editor-actions">
          <div>
            <a-button :loading="formatting" :disabled="loading || saving" @click="$emit('format')">
              <template #icon><icon-brush /></template>
              {{ t("格式化并检查") }}
            </a-button>
            <a-button :disabled="saving" @click="$emit('reload')">
              <template #icon><icon-refresh /></template>
              {{ t("重新加载") }}
            </a-button>
          </div>
          <div>
            <a-button :disabled="saving" @click="$emit('update:visible', false)">
              {{ t("取消") }}
            </a-button>
            <a-button type="primary" :loading="saving" :disabled="loading" @click="$emit('save')">
              <template #icon><icon-save /></template>
              {{ t("保存文件") }}
            </a-button>
          </div>
        </div>
      </div>
    </a-spin>
  </a-modal>
</template>
