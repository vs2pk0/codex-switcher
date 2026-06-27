export interface BadgeAccountType {
  key: string;
  label: string;
  planClass: string;
  defaultStyle: string;
}

export interface BadgeStyleOption {
  value: string;
  label: string;
  icon: string;
}

export const BADGE_ACCOUNT_TYPES: BadgeAccountType[] = [
  { key: "free", label: "FREE", planClass: "free", defaultStyle: "gold" },
  { key: "plus", label: "PLUS", planClass: "plus", defaultStyle: "amber" },
  { key: "proLite", label: "PRO 5X", planClass: "pro-lite", defaultStyle: "violet" },
  { key: "proMax", label: "PRO 20X", planClass: "pro-max", defaultStyle: "cyan" },
  { key: "team", label: "TEAM", planClass: "team", defaultStyle: "emerald" },
  { key: "api", label: "API_KEY", planClass: "api", defaultStyle: "stamp" },
];

export const BADGE_STYLE_OPTIONS: BadgeStyleOption[] = [
  { value: "classic", label: "经典", icon: "crown" },
  { value: "stamp", label: "战术印章", icon: "badge-check" },
  { value: "neon", label: "霓虹裂变", icon: "sparkles" },
  { value: "amber", label: "琥珀闪电", icon: "bolt" },
  { value: "violet", label: "轻核紫钻", icon: "gem" },
  { value: "cyan", label: "星翼火箭", icon: "rocket" },
  { value: "emerald", label: "晶格盾章", icon: "shield-check" },
  { value: "rose", label: "赤羽火焰", icon: "flame" },
  { value: "slate", label: "黑曜轻翼", icon: "diamond" },
  { value: "gold", label: "鎏金王冠", icon: "crown" },
  { value: "quantum", label: "量子矩阵", icon: "cpu" },
  { value: "plasma", label: "等离子刃", icon: "zap" },
  { value: "void", label: "虚空黑曜", icon: "orbit" },
  { value: "ion", label: "离子脉冲", icon: "radio" },
  { value: "nova", label: "超新星核", icon: "star" },
  { value: "drake", label: "焰龙熔铸", icon: "flame" },
  { value: "cyber", label: "赛博棱镜", icon: "circuit" },
  { value: "titan", label: "泰坦重甲", icon: "hexagon" },
  { value: "aurora", label: "极光镭射", icon: "aperture" },
  { value: "meteor", label: "陨星冲击", icon: "satellite" },
  { value: "obsidian", label: "黑金裁决", icon: "gem" },
  { value: "glacier", label: "冰川蓝焰", icon: "shield-check" },
  { value: "matrix", label: "矩阵绿潮", icon: "binary" },
  { value: "solar", label: "日冕爆燃", icon: "bolt" },
  { value: "lunar", label: "月蚀银弧", icon: "radar" },
  { value: "rift", label: "裂隙紫电", icon: "network" },
  { value: "apex", label: "巅峰红芯", icon: "trophy" },
  { value: "omega", label: "欧米伽环", icon: "orbit" },
  { value: "vertex", label: "顶点蓝晶", icon: "aperture" },
  { value: "zero", label: "零点推进", icon: "zap" },
];

export const BADGE_STYLE_ICON_KEYS = Object.fromEntries(
  BADGE_STYLE_OPTIONS.map((style) => [style.value, style.icon]),
);

export function defaultBadgeStyles(): Record<string, string> {
  return Object.fromEntries(BADGE_ACCOUNT_TYPES.map((type) => [type.key, type.defaultStyle]));
}
