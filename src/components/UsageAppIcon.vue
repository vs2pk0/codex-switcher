<script setup lang="ts">
import OpenAiIcon from "./OpenAiIcon.vue";

defineProps<{ app: string; iconPng?: string }>();
</script>

<template>
  <OpenAiIcon v-if="app === 'codex'" class="usage-app-icon" />
  <img v-else-if="iconPng" :src="`data:image/png;base64,${iconPng}`" alt="" class="usage-app-icon" />
  <icon-code v-else-if="app" class="usage-app-icon" aria-hidden="true" />
  <icon-apps v-else class="usage-app-icon" aria-hidden="true" />
</template>

<style scoped>
.usage-app-icon {
  width: 1em;
  height: 1em;
  flex: none;
  object-fit: contain;
}
</style>
