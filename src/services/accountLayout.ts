/** 每行账号数设置值：0 表示自适应，其余为固定列数。 */
export type AccountColumnsSetting = 0 | 3 | 4 | 5;

/** 自适应的基准：1800px 宽的窗口正好显示 5 列。 */
export const ADAPTIVE_REFERENCE_WINDOW_WIDTH = 1800;
export const ADAPTIVE_REFERENCE_COLUMNS = 5;
/** 自适应时至少保留 3 列，避免窄窗口下卡片过宽。 */
export const ADAPTIVE_MIN_COLUMNS = 3;
/** 自适应时最多显示的列数，超过后卡片过窄无法完整展示信息。 */
export const ADAPTIVE_MAX_COLUMNS = 8;

export const ADAPTIVE_COLUMNS = 0 as const;
export const FIXED_COLUMN_OPTIONS: readonly AccountColumnsSetting[] = [3, 4, 5];

/** 将持久化的设置值归一化为合法的每行账号数，非法值回退为自适应。 */
export function normalizeMaxColumns(value: unknown): AccountColumnsSetting {
  const numeric = typeof value === "number" ? value : Number(value);
  if (numeric === ADAPTIVE_COLUMNS) return ADAPTIVE_COLUMNS;
  return FIXED_COLUMN_OPTIONS.includes(numeric as AccountColumnsSetting)
    ? (numeric as AccountColumnsSetting)
    : ADAPTIVE_COLUMNS;
}

/** 按窗口宽度计算自适应列数：以 1800px 显示 5 列（每列 360px）为基准向下取整。 */
export function resolveAdaptiveColumns(windowWidth: number): number {
  if (!Number.isFinite(windowWidth) || windowWidth <= 0) return ADAPTIVE_REFERENCE_COLUMNS;
  const columnUnit = ADAPTIVE_REFERENCE_WINDOW_WIDTH / ADAPTIVE_REFERENCE_COLUMNS;
  const columns = Math.floor(windowWidth / columnUnit);
  return Math.min(ADAPTIVE_MAX_COLUMNS, Math.max(ADAPTIVE_MIN_COLUMNS, columns));
}

/** 综合设置值与窗口宽度，得到实际渲染的列数。 */
export function resolveAccountColumns(setting: unknown, windowWidth: number): number {
  const normalized = normalizeMaxColumns(setting);
  return normalized === ADAPTIVE_COLUMNS ? resolveAdaptiveColumns(windowWidth) : normalized;
}
