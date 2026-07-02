<script setup lang="ts">
import { t } from "../i18n";
import appIcon from "../assets/app-icon.png";

defineProps<{
  appVersion: string;
}>();

defineEmits<{
  (event: "open-github-profile"): void;
  (event: "open-repository"): void;
  (event: "open-sponsor-page"): void;
  (event: "open-feedback-page"): void;
  (event: "check-update"): void;
}>();
</script>

<template>
  <section class="about-panel">
    <div class="about-hero">
      <div class="about-icon-wrap">
        <img :src="appIcon" alt="Codex Switcher" class="about-app-icon" />
      </div>
      <span class="about-kicker">{{ t("本地 Codex 管理工具") }}</span>
      <h2>Codex Switcher</h2>
      <div class="about-badges">
        <span class="about-version">v{{ appVersion }}</span>
        <a-button class="about-github-button" size="small" @click="$emit('open-repository')">
          <template #icon><icon-link /></template>
          GitHub
        </a-button>
      </div>
      <p>{{ t("把账号切换、会话维护、使用统计和本地 API 服务收在一个干净的桌面工作台里。") }}</p>
    </div>

    <div class="about-grid">
      <button type="button" class="about-tile" @click="$emit('open-github-profile')">
        <span class="about-tile-icon about-tile-icon-profile">
          <icon-user />
        </span>
        <span class="about-tile-copy">
          <strong>{{ t("作者主页") }}</strong>
          <span>{{ t("访问 vs2pk0 的 GitHub 主页") }}</span>
        </span>
      </button>
      <button type="button" class="about-tile" @click="$emit('open-repository')">
        <span class="about-tile-icon about-tile-icon-repo">
          <icon-code />
        </span>
        <span class="about-tile-copy">
          <strong>{{ t("开源仓库") }}</strong>
          <span>{{ t("查看源码、版本和发布记录") }}</span>
        </span>
      </button>
      <button type="button" class="about-tile" @click="$emit('open-sponsor-page')">
        <span class="about-tile-icon about-tile-icon-sponsor">
          <icon-heart />
        </span>
        <span class="about-tile-copy">
          <strong>{{ t("赞助支持") }}</strong>
          <span>{{ t("查看支付宝、微信和 Binance 收款码") }}</span>
        </span>
      </button>
      <button type="button" class="about-tile" @click="$emit('open-feedback-page')">
        <span class="about-tile-icon about-tile-icon-feedback">
          <icon-message />
        </span>
        <span class="about-tile-copy">
          <strong>{{ t("问题反馈") }}</strong>
          <span>{{ t("提交 Issue 或改进建议") }}</span>
        </span>
      </button>
      <button type="button" class="about-tile" @click="$emit('check-update')">
        <span class="about-tile-icon about-tile-icon-update">
          <icon-refresh />
        </span>
        <span class="about-tile-copy">
          <strong>{{ t("检查更新") }}</strong>
          <span>{{ t("查看最新版本并下载安装包") }}</span>
        </span>
      </button>
    </div>
  </section>
</template>
