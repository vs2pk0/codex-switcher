export type CodexAuthMode = "apikey" | string | undefined;

export interface CodexTokens {
  id_token: string;
  access_token: string;
  refresh_token?: string;
}

export interface CodexAccount {
  id: string;
  email: string;
  account_name?: string;
  auth_mode?: CodexAuthMode;
  openai_api_key?: string;
  openaiApiKey?: string;
  api_base_url?: string;
  apiBaseUrl?: string;
  api_provider_name?: string;
  apiProviderName?: string;
  api_official_url?: string;
  apiOfficialUrl?: string;
  plan_type?: string;
  auth_file_plan_type?: string;
  bound_oauth_account_id?: string | null;
  bound_oauth_use_local_gateway?: boolean;
  bound_phone?: string;
  subscription_active_until?: string;
  access_token_expires_at?: string;
  quota?: CodexQuota;
  quota_error?: CodexQuotaErrorInfo;
  usage_updated_at?: number;
  tokens: CodexTokens;
  created_at: number;
  last_used: number;
}

export interface CodexQuotaErrorInfo {
  code?: string;
  message: string;
  timestamp: number;
}

export interface CodexQuota {
  hourly_percentage: number;
  hourly_reset_time?: number;
  hourly_window_minutes?: number;
  hourly_window_present?: boolean;
  weekly_percentage: number;
  weekly_reset_time?: number;
  weekly_window_minutes?: number;
  weekly_window_present?: boolean;
  reset_credits_available?: number;
  raw_data?: unknown;
}
