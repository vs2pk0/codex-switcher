<script setup lang="ts">
import { computed } from "vue";
import { BADGE_STYLE_ICON_KEYS } from "../constants/badgeStyles";

const props = defineProps<{
  label: string;
  badgeClass: string | string[];
}>();

const classList = computed(() => {
  return Array.isArray(props.badgeClass) ? props.badgeClass : props.badgeClass.split(/\s+/);
});

const styleName = computed(() => {
  const styleClass = classList.value.find((item) => item.startsWith("badge-"));
  return styleClass?.replace(/^badge-/, "") || "classic";
});

const iconName = computed(() => BADGE_STYLE_ICON_KEYS[styleName.value] || "crown");

const iconPaths: Record<string, string[]> = {
  aperture: [
    "M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2Z",
    "M12 2 7.5 10M21.2 8H12M16.5 20 12 12M2.8 16H12M7.5 4 12 12",
  ],
  "badge-check": [
    "m15.5 7.5 2.8.2 1 2.6 2.1 1.7-1.1 2.6.2 2.8-2.6 1-1.7 2.1-2.6-1.1-2.8.2-1-2.6-2.1-1.7 1.1-2.6-.2-2.8 2.6-1 1.7-2.1 2.6 1.1Z",
    "m12.2 14.2 2 2 4-4",
  ],
  badge: ["M3.85 8.62a4 4 0 0 1 4.78-4.77 4 4 0 0 1 6.74 0 4 4 0 0 1 4.78 4.78 4 4 0 0 1 0 6.74 4 4 0 0 1-4.78 4.78 4 4 0 0 1-6.74 0 4 4 0 0 1-4.78-4.78 4 4 0 0 1 0-6.75Z"],
  binary: ["M6 20h4M14 20h4M6 4h4M14 4h4M8 4v16M16 4v16"],
  bolt: ["M13 2 3 14h9l-1 8 10-12h-9l1-8Z"],
  "brain-circuit": ["M12 5a3 3 0 0 0-5.9.8A3.5 3.5 0 0 0 4 12.3 3.5 3.5 0 0 0 7 18a3 3 0 0 0 5 1M12 5a3 3 0 0 1 5.9.8A3.5 3.5 0 0 1 20 12.3 3.5 3.5 0 0 1 17 18a3 3 0 0 1-5 1M12 5v14M8 9h2M14 9h2M8 15h2M14 15h2"],
  "circle-dollar": ["M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Z", "M16 8h-6a2 2 0 0 0 0 4h4a2 2 0 0 1 0 4H8M12 18V6"],
  circuit: [
    "M7 7h10v10H7z",
    "M12 2v5M12 17v5M2 12h5M17 12h5M4.9 4.9 8.4 8.4M15.6 8.4l3.5-3.5M4.9 19.1l3.5-3.5M15.6 15.6l3.5 3.5",
  ],
  "circuit-board": [
    "M7 7h10v10H7z",
    "M12 2v5M12 17v5M2 12h5M17 12h5M7 12H5a3 3 0 0 1-3-3V7M17 12h2a3 3 0 0 0 3-3V7M7 12H5a3 3 0 0 0-3 3v2M17 12h2a3 3 0 0 1 3 3v2",
  ],
  crown: [
    "M2 6l5 5 5-9 5 9 5-5-2 14H4L2 6Z",
    "M4 20h16",
  ],
  cpu: ["M9 2v3M15 2v3M9 19v3M15 19v3M2 9h3M2 15h3M19 9h3M19 15h3", "M7 7h10v10H7z", "M9 9h6v6H9z"],
  diamond: ["M6 3h12l4 6-10 12L2 9l4-6Z", "M2 9h20"],
  fingerprint: ["M12 11a2 2 0 0 1 2 2c0 1.5-.4 3-.9 4.4M8.1 18.6c.6-1.4.9-3 .9-4.6a3 3 0 0 1 6 0c0 .7-.1 1.6-.3 2.5M18 14a6 6 0 0 0-12 0M6.2 19.8C5.4 18 5 16 5 14a7 7 0 0 1 14 0c0 2-.4 4.1-1.2 5.9M9 5.5A8 8 0 0 1 20 13"],
  flame: ["M8.5 14.5A2.5 2.5 0 0 0 11 17c1.4 0 2.5-1.1 2.5-2.5 0-1.2-.7-2-1.5-2.8-.9.8-1.6 1.6-1.9 2.8", "M12 22c4-1 7-4 7-8.5 0-3.6-2.4-6.3-5.8-8.6C13 7 12 8.8 10 10 9.6 7.5 7.8 5.4 5.5 4 6 7.5 3 9.8 3 14c0 4.4 3.6 8 9 8Z"],
  gem: [
    "M6 3h12l4 6-10 12L2 9l4-6Z",
    "M2 9h20M6 3l6 18 6-18",
  ],
  hexagon: ["M21 16V8l-9-5-9 5v8l9 5 9-5Z"],
  medal: ["M7 2h10l-2 6H9L7 2Z", "M12 8a6 6 0 1 0 0 12 6 6 0 0 0 0-12Z", "m10 14 1.2 1.2L14 12.5"],
  network: [
    "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8ZM4 4a2 2 0 1 0 0 4 2 2 0 0 0 0-4ZM20 4a2 2 0 1 0 0 4 2 2 0 0 0 0-4ZM4 16a2 2 0 1 0 0 4 2 2 0 0 0 0-4ZM20 16a2 2 0 1 0 0 4 2 2 0 0 0 0-4Z",
    "m6 7 3 2.5M18 7l-3 2.5M6 17l3-2.5M18 17l-3-2.5",
  ],
  orbit: [
    "M12 12h.01",
    "M19.1 4.9c2 2 0 7.2-4.4 11.6s-9.6 6.4-11.6 4.4 0-7.2 4.4-11.6 9.6-6.4 11.6-4.4Z",
    "M4.9 4.9c-2 2 0 7.2 4.4 11.6s9.6 6.4 11.6 4.4 0-7.2-4.4-11.6-9.6-6.4-11.6-4.4Z",
  ],
  radar: [
    "M12 12 20 4",
    "M12 2a10 10 0 1 0 10 10M12 6a6 6 0 1 0 6 6",
  ],
  radio: [
    "M4.9 19.1a10 10 0 0 1 0-14.2M19.1 4.9a10 10 0 0 1 0 14.2M8 16a5 5 0 0 1 0-8M16 8a5 5 0 0 1 0 8",
    "M12 12h.01M12 12v8",
  ],
  rocket: [
    "M4.5 16.5 3 21l4.5-1.5M7 14l3 3M9 15l6-6c2.5-2.5 4.5-3.5 7-4-.5 2.5-1.5 4.5-4 7l-6 6-5-5Z",
    "M15 9h.01",
  ],
  satellite: [
    "m13 7 4 4M10 10l4 4M7 13l4 4",
    "M5 3 21 19M5 21a8 8 0 0 1 8-8",
  ],
  "scan-line": ["M3 7V5a2 2 0 0 1 2-2h2M17 3h2a2 2 0 0 1 2 2v2M21 17v2a2 2 0 0 1-2 2h-2M7 21H5a2 2 0 0 1-2-2v-2M7 12h10"],
  shield: ["M12 2 20 6v6c0 5-3.4 8.4-8 10-4.6-1.6-8-5-8-10V6l8-4Z"],
  "shield-check": [
    "M12 2 20 6v6c0 5-3.4 8.4-8 10-4.6-1.6-8-5-8-10V6l8-4Z",
    "m9 12 2 2 4-4",
  ],
  sparkles: [
    "M12 3 14 9l6 2-6 2-2 6-2-6-6-2 6-2 2-6Z",
    "M19 3v4M21 5h-4M5 17v4M7 19H3",
  ],
  star: ["m12 2 3.1 6.3 6.9 1-5 4.9 1.2 6.8L12 17.8 5.8 21 7 14.2l-5-4.9 6.9-1L12 2Z"],
  trophy: [
    "M8 21h8M12 17v4M7 4h10v5a5 5 0 0 1-10 0V4Z",
    "M7 5H4v2a4 4 0 0 0 4 4M17 5h3v2a4 4 0 0 1-4 4",
  ],
  "wand-sparkles": ["M15 4V2M15 8v-2M13 4h4M5 3l14 14-3 3L2 6l3-3Z", "M19 11v-2M19 15v-2M17 13h4"],
  webhook: ["M18 16.5a3.5 3.5 0 1 0-3.1-5.1L12 16.5a3.5 3.5 0 1 1-3-5.2M6 7.5a3.5 3.5 0 1 1 5.7 2.7L15 16.5"],
  zap: ["M13 2 3 14h9l-1 8 10-12h-9l1-8Z"],
};

const paths = computed(() => iconPaths[iconName.value] ?? iconPaths.crown);
</script>

<template>
  <span class="plan-badge" :class="badgeClass" :title="label" :aria-label="label">
    <span class="plan-badge-icon-shell" aria-hidden="true">
      <svg viewBox="0 0 24 24" focusable="false">
        <path
          v-for="(path, index) in paths"
          :key="index"
          :d="path"
          fill="none"
          stroke="currentColor"
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
        />
      </svg>
    </span>
    <span class="plan-badge-label">{{ label }}</span>
  </span>
</template>
