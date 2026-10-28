export interface ApiModelSettings {
  autoCompactEnabled: boolean;
  autoCompactTokenLimit: number;
  reasoningEnabled: boolean;
  reasoningLevels: string[];
  reasoningEffort: string;
  disableWebSearch: boolean;
  fastMode: boolean;
}

export const reasoningEffortOptions = ["none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"];
export const maxAutoCompactTokenLimit = 2_147_483_647;

export function defaultApiModelSettings(): ApiModelSettings {
  return {
    autoCompactEnabled: false,
    autoCompactTokenLimit: 180_000,
    reasoningEnabled: false,
    reasoningLevels: ["low", "medium", "high", "xhigh"],
    reasoningEffort: "high",
    disableWebSearch: false,
    fastMode: false,
  };
}

export function normalizeApiModelSettings(value?: Partial<ApiModelSettings>): ApiModelSettings {
  const defaults = defaultApiModelSettings();
  return { ...defaults, ...value, reasoningLevels: [...(value?.reasoningLevels ?? defaults.reasoningLevels)] };
}

export function validateApiModelSettings(value: ApiModelSettings): string | null {
  if (value.autoCompactEnabled && (!Number.isInteger(value.autoCompactTokenLimit)
    || value.autoCompactTokenLimit < 1 || value.autoCompactTokenLimit > maxAutoCompactTokenLimit)) {
    return "自动压缩阈值必须为 1 到 2147483647 之间的整数";
  }
  if (value.reasoningEnabled && (!value.reasoningLevels.length || value.reasoningLevels.length > 16
    || value.reasoningLevels.some(level => !/^[a-z0-9_]{1,32}$/.test(level))
    || new Set(value.reasoningLevels).size !== value.reasoningLevels.length
    || !value.reasoningLevels.includes(value.reasoningEffort))) {
    return "请选择有效且不重复的思考档位，并将默认强度设为其中一档";
  }
  return null;
}
