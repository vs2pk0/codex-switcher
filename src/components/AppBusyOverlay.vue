<script setup lang="ts">
// 全局忙碌遮罩：用于一键修复等长耗时流程，覆盖整个窗口并拦截所有交互，
// 避免用户在修复途中切换页面或触发其他操作。
defineProps<{
  visible: boolean;
  title: string;
  message?: string;
  steps?: string[];
  activeStep?: number;
}>();
</script>

<template>
  <Teleport to="body">
    <Transition name="app-busy-fade">
      <div
        v-if="visible"
        class="app-busy-overlay"
        role="alertdialog"
        aria-busy="true"
        aria-live="polite"
        @keydown.stop.prevent
        @wheel.prevent
      >
        <div class="app-busy-card">
          <a-spin dot :size="32" />
          <strong class="app-busy-title">{{ title }}</strong>
          <p v-if="message" class="app-busy-message">{{ message }}</p>
          <ol v-if="steps?.length" class="app-busy-steps">
            <li
              v-for="(step, index) in steps"
              :key="step"
              :class="{
                done: activeStep !== undefined && index < activeStep,
                active: activeStep === index,
              }"
            >
              <span class="app-busy-step-dot" />
              <span>{{ step }}</span>
            </li>
          </ol>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
