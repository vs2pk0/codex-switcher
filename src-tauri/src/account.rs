use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;
use toml_edit::{value, Document, Item};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodexTokens {
    pub id_token: String,
    pub access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodexQuota {
    pub hourly_percentage: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hourly_reset_time: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hourly_window_minutes: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hourly_window_present: Option<bool>,
    pub weekly_percentage: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weekly_reset_time: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weekly_window_minutes: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weekly_window_present: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_credits_available: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reset_credits: Vec<CodexResetCredit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_credits_next_expires_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodexResetCredit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub granted_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redeemed_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodexQuotaErrorInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub message: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodexAccount {
    pub id: String,
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openai_api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_provider_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_official_url: Option<String>,
    #[serde(
        default,
        alias = "defaultModel",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_file_plan_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bound_oauth_account_id: Option<String>,
    #[serde(default)]
    pub bound_oauth_use_local_gateway: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bound_phone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscription_active_until: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token_expires_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quota: Option<CodexQuota>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quota_error: Option<CodexQuotaErrorInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_updated_at: Option<i64>,
    pub tokens: CodexTokens,
    pub created_at: i64,
    pub last_used: i64,
}

#[derive(Debug, Clone)]
pub struct ApiKeyAccountBindingInput {
    pub api_key: String,
    pub api_base_url: Option<String>,
    pub api_provider_name: Option<String>,
    pub api_official_url: Option<String>,
    pub account_name: Option<String>,
    pub bound_oauth_account_id: Option<String>,
    pub bound_oauth_use_local_gateway: bool,
}

#[derive(Debug, Clone)]
pub struct AccountStore {
    storage_dir: PathBuf,
    codex_home: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AccountDatabase {
    #[serde(default)]
    accounts: Vec<CodexAccount>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    current_account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    managed_model_config_backup: Option<ManagedModelConfigBackup>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ManagedModelConfigBackup {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    managed_account_id: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    root_items: BTreeMap<String, String>,
    #[serde(default)]
    features_table_existed: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    feature_items: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    provider_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    provider_items: BTreeMap<String, BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    touched_provider_ids: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct CodexCurrentConfig {
    api_key: Option<String>,
    provider_base_url: Option<String>,
    provider_api_key: Option<String>,
    oauth_email: Option<String>,
    oauth_access_token: Option<String>,
}

fn clear_account_quota(account: &mut CodexAccount) {
    account.quota = None;
    account.quota_error = None;
    account.usage_updated_at = None;
}

fn select_account_value_for_update<'a>(
    value: &'a Value,
    old_account: &CodexAccount,
) -> Result<&'a Value, String> {
    let accounts = match value {
        Value::Array(items) => Some(items),
        _ => value.get("accounts").and_then(Value::as_array),
    };
    let Some(accounts) = accounts else {
        return Ok(value);
    };
    if accounts.is_empty() {
        return Err("导出 JSON 中没有账号数据".to_string());
    }
    if let Some(account) = accounts.iter().find(|candidate| {
        read_string(candidate, &["id", "account_id", "accountId"]).as_deref()
            == Some(old_account.id.as_str())
    }) {
        return Ok(account);
    }
    if let Some(account) = accounts.iter().find(|candidate| {
        read_string(candidate, &["email"])
            .is_some_and(|email| email.eq_ignore_ascii_case(&old_account.email))
    }) {
        return Ok(account);
    }
    if accounts.len() == 1 {
        return Ok(&accounts[0]);
    }
    Err("导出 JSON 中包含多个账号，未找到当前编辑账号".to_string())
}

impl Default for AccountStore {
    fn default() -> Self {
        let root_dir = switcher_root_dir();
        let data_root = dirs::data_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let data_dir = migrate_legacy_storage_dir(
            root_dir.join("account"),
            &[
                data_root.join("codex-switcher"),
                data_root.join("codex-account-switcher"),
                data_root.join(["codex", "account", "switcher"].join("-")),
            ],
        );
        let codex_home = std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|home| home.join(".codex")))
            .unwrap_or_else(|| PathBuf::from(".codex"));
        Self::new(data_dir, codex_home)
    }
}

fn switcher_root_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join(".codex_switcher")
}

fn migrate_legacy_storage_dir(next_dir: PathBuf, legacy_dirs: &[PathBuf]) -> PathBuf {
    if next_dir.join("accounts.json").exists() {
        return next_dir;
    }
    if let Some(legacy_dir) = legacy_dirs
        .iter()
        .find(|dir| dir.join("accounts.json").exists())
    {
        let _ = fs::create_dir_all(&next_dir);
        let _ = fs::copy(
            legacy_dir.join("accounts.json"),
            next_dir.join("accounts.json"),
        );
    }
    next_dir
}

impl AccountStore {
    pub fn new(storage_dir: PathBuf, codex_home: PathBuf) -> Self {
        Self {
            storage_dir,
            codex_home,
        }
    }

    pub fn list_accounts(&self) -> Result<Vec<CodexAccount>, String> {
        Ok(self.read_database()?.accounts)
    }

    pub fn current_account(&self) -> Result<Option<CodexAccount>, String> {
        let database = self.read_database()?;
        let Some(current_id) = database.current_account_id else {
            return Ok(None);
        };
        Ok(database
            .accounts
            .into_iter()
            .find(|account| account.id == current_id))
    }

    pub fn detect_current_account_from_codex_config(&self) -> Result<Option<CodexAccount>, String> {
        let mut database = self.read_database()?;
        if database.accounts.is_empty() {
            return Ok(None);
        }
        let config = self.read_current_codex_config()?;
        let Some(account) = match_current_account_from_config(&database.accounts, &config) else {
            return Ok(None);
        };
        if database
            .managed_model_config_backup
            .as_ref()
            .and_then(|backup| backup.managed_account_id.as_deref())
            != Some(account.id.as_str())
        {
            database.managed_model_config_backup = None;
        }
        database.current_account_id = Some(account.id.clone());
        self.write_database(&database)?;
        Ok(Some(account))
    }

    pub fn import_from_json(&self, json_content: &str) -> Result<Vec<CodexAccount>, String> {
        let trimmed = json_content.trim();
        if trimmed.is_empty() {
            return Err("请输入 Token 或 JSON".to_string());
        }

        let parsed: Value =
            serde_json::from_str(trimmed).unwrap_or_else(|_| Value::String(trimmed.to_string()));
        let parsed = parsed
            .get("accounts")
            .filter(|accounts| accounts.is_array())
            .cloned()
            .unwrap_or(parsed);
        let mut imported = Vec::new();
        match parsed {
            Value::Array(items) => {
                for item in items {
                    imported.push(self.account_from_import_value(&item)?);
                }
            }
            value => imported.push(self.account_from_import_value(&value)?),
        }

        let mut database = self.read_database()?;
        for account in imported.iter().cloned() {
            upsert_account(&mut database.accounts, account);
        }
        self.write_database(&database)?;
        Ok(imported)
    }

    pub fn import_from_local(&self) -> Result<Vec<CodexAccount>, String> {
        let auth_path = self.codex_home.join("auth.json");
        if !auth_path.exists() {
            return Err(format!(
                "未找到本机 Codex 账号文件：{}",
                auth_path.display()
            ));
        }
        let content = fs::read_to_string(&auth_path)
            .map_err(|error| format!("读取 auth.json 失败: {}", error))?;
        self.import_from_json(&content)
    }

    pub fn save_oauth_tokens(
        &self,
        id_token: String,
        access_token: String,
        refresh_token: Option<String>,
    ) -> Result<CodexAccount, String> {
        let payload = serde_json::json!({
            "tokens": {
                "id_token": id_token,
                "access_token": access_token,
                "refresh_token": refresh_token.unwrap_or_default()
            }
        });
        let mut account = self.account_from_import_value(&payload)?;
        let mut database = self.read_database()?;
        let current_id = database.current_account_id.clone();
        let existing = database
            .accounts
            .iter()
            .find(|candidate| {
                candidate.auth_mode.as_deref() != Some("apikey")
                    && candidate.email.eq_ignore_ascii_case(&account.email)
            })
            .cloned();
        let was_current = existing
            .as_ref()
            .is_some_and(|existing| current_id.as_deref() == Some(existing.id.as_str()))
            || current_id.as_deref() == Some(account.id.as_str());
        let current_api_key_depends_on_refreshed_oauth =
            existing.as_ref().is_some_and(|existing| {
                database.accounts.iter().any(|candidate| {
                    current_id.as_deref() == Some(candidate.id.as_str())
                        && candidate.auth_mode.as_deref() == Some("apikey")
                        && candidate.bound_oauth_account_id.as_deref() == Some(existing.id.as_str())
                })
            });
        if let Some(existing) = existing {
            account.id = existing.id;
            account.account_name = existing.account_name;
            account.bound_phone = existing.bound_phone;
            account.created_at = existing.created_at;
            account.last_used = existing.last_used;
        }
        upsert_account(&mut database.accounts, account.clone());
        if was_current {
            database.current_account_id = Some(account.id.clone());
            write_codex_auth_projection(
                &self.codex_home,
                &account,
                &database.accounts,
                ManagedModelTransition::default(),
            )?;
        } else if current_api_key_depends_on_refreshed_oauth {
            if let Some(current) = database
                .accounts
                .iter()
                .find(|candidate| current_id.as_deref() == Some(candidate.id.as_str()))
                .cloned()
            {
                write_codex_auth_projection(
                    &self.codex_home,
                    &current,
                    &database.accounts,
                    ManagedModelTransition::default(),
                )?;
            }
        }
        self.write_database(&database)?;
        Ok(account)
    }

    pub fn add_api_key_account(
        &self,
        api_key: String,
        api_base_url: Option<String>,
        api_provider_name: Option<String>,
        api_official_url: Option<String>,
        account_name: Option<String>,
    ) -> Result<CodexAccount, String> {
        let account = build_api_key_account_record(
            api_key,
            api_base_url,
            api_provider_name,
            api_official_url,
            account_name,
        )?;

        let mut database = self.read_database()?;
        upsert_account(&mut database.accounts, account.clone());
        self.write_database(&database)?;
        Ok(account)
    }

    pub fn add_api_key_account_with_binding(
        &self,
        input: ApiKeyAccountBindingInput,
    ) -> Result<CodexAccount, String> {
        let account = self.add_api_key_account(
            input.api_key,
            input.api_base_url,
            input.api_provider_name,
            input.api_official_url,
            input.account_name,
        )?;
        if normalize_optional(input.bound_oauth_account_id.as_deref()).is_some() {
            self.update_api_key_bound_oauth_account(
                &account.id,
                input.bound_oauth_account_id,
                input.bound_oauth_use_local_gateway,
            )
        } else {
            Ok(account)
        }
    }

    pub fn update_api_key_credentials(
        &self,
        account_id: &str,
        api_key: String,
        api_base_url: Option<String>,
        api_provider_name: Option<String>,
        api_official_url: Option<String>,
    ) -> Result<CodexAccount, String> {
        let (api_key, api_base_url) =
            validate_api_key_credentials(&api_key, api_base_url.as_deref())?;
        let api_official_url = validate_optional_url(api_official_url.as_deref(), "官网地址")?;
        let mut database = self.read_database()?;
        let account = database
            .accounts
            .iter_mut()
            .find(|account| account.id == account_id)
            .ok_or_else(|| "账号不存在".to_string())?;

        if account.auth_mode.as_deref() != Some("apikey") {
            return Err("只有 API Key 账号可以更新 API Key".to_string());
        }

        account.openai_api_key = Some(api_key);
        account.api_base_url = api_base_url;
        account.api_provider_name = normalize_optional(api_provider_name.as_deref());
        account.api_official_url = api_official_url;
        account.last_used = now_timestamp();
        let updated = account.clone();
        self.write_database(&database)?;
        Ok(updated)
    }

    pub fn update_api_key_default_model(
        &self,
        account_id: &str,
        model_id: String,
    ) -> Result<CodexAccount, String> {
        let model_id = validate_default_model(&model_id)?;
        let mut database = self.read_database()?;
        let account_index = database
            .accounts
            .iter()
            .position(|account| account.id == account_id)
            .ok_or_else(|| "账号不存在".to_string())?;
        if database.accounts[account_index].auth_mode.as_deref() != Some("apikey") {
            return Err("只有 API Key 账号可以设置默认模型".to_string());
        }

        let current_config = self.read_current_codex_config()?;
        if !strict_api_key_model_access_matches(
            &database.accounts,
            &database.accounts[account_index],
            &current_config,
        ) {
            return Err("当前 Codex 配置不是该 API Key 账号，请先切换到该账号后重试".to_string());
        }

        if database
            .managed_model_config_backup
            .as_ref()
            .and_then(|backup| backup.managed_account_id.as_deref())
            != Some(account_id)
        {
            database.managed_model_config_backup = None;
        }
        database.current_account_id = Some(account_id.to_string());

        let mut previous_model = database.accounts[account_index].default_model.clone();
        let config_path = self.codex_home.join("config.toml");
        let config_before_update = fs::read_to_string(&config_path)
            .map_err(|error| format!("读取 config.toml 失败: {}", error))?;
        let config_document = read_toml_document(&config_path)?;
        if previous_model
            .as_deref()
            .is_some_and(|model| config_document.get("model").and_then(Item::as_str) != Some(model))
        {
            database.managed_model_config_backup = None;
            previous_model = None;
        }
        let previous_model_was_gpt_5_6 = is_gpt_5_6_model(previous_model.as_deref());
        let next_model_is_gpt_5_6 = is_gpt_5_6_model(Some(&model_id));
        database.accounts[account_index].default_model = Some(model_id);
        database.accounts[account_index].last_used = now_timestamp();
        if database.managed_model_config_backup.is_none() {
            database.managed_model_config_backup =
                Some(capture_managed_model_config(&config_document, account_id));
        }
        let provider_id = api_key_provider_id(&database.accounts[account_index]);
        if let Some(backup) = database.managed_model_config_backup.as_mut() {
            backup.managed_account_id = Some(account_id.to_string());
            if next_model_is_gpt_5_6 && !backup.touched_provider_ids.contains(&provider_id) {
                backup.touched_provider_ids.push(provider_id);
            }
        }
        let updated = database.accounts[account_index].clone();
        write_api_key_provider_config(
            &self.codex_home,
            &updated,
            ManagedModelTransition {
                restore_model: false,
                restore_gpt_5_6: previous_model_was_gpt_5_6 && !next_model_is_gpt_5_6,
                expected_model: previous_model,
                restore_provider_ids: database
                    .managed_model_config_backup
                    .as_ref()
                    .map(|backup| backup.touched_provider_ids.clone())
                    .filter(|ids| !ids.is_empty())
                    .unwrap_or_else(|| {
                        if previous_model_was_gpt_5_6 {
                            vec![api_key_provider_id(&updated)]
                        } else {
                            Vec::new()
                        }
                    }),
                backup: database.managed_model_config_backup.clone(),
            },
        )?;
        rollback_config_on_error(
            self.write_database(&database),
            &config_path,
            &config_before_update,
        )?;
        Ok(updated)
    }

    pub fn check_api_key_model_access(&self, account_id: &str) -> Result<bool, String> {
        let database = self.read_database()?;
        let Some(account) = database
            .accounts
            .iter()
            .find(|account| account.id == account_id)
        else {
            return Err("账号不存在".to_string());
        };
        if account.auth_mode.as_deref() != Some("apikey") {
            return Ok(false);
        }
        let current_config = self.read_current_codex_config()?;
        Ok(strict_api_key_model_access_matches(
            &database.accounts,
            account,
            &current_config,
        ))
    }

    pub fn release_current_api_key_default_model(&self) -> Result<bool, String> {
        let mut database = self.read_database()?;
        let mut changed = database.managed_model_config_backup.take().is_some();
        if let Some(current_id) = database.current_account_id.as_deref() {
            if let Some(account) = database.accounts.iter_mut().find(|account| {
                account.id == current_id && account.auth_mode.as_deref() == Some("apikey")
            }) {
                changed |= account.default_model.take().is_some();
            }
        }
        if changed {
            self.write_database(&database)?;
        }
        Ok(changed)
    }

    pub fn update_account_profile(
        &self,
        account_id: &str,
        account_name: Option<String>,
    ) -> Result<CodexAccount, String> {
        let mut database = self.read_database()?;
        let account = database
            .accounts
            .iter_mut()
            .find(|account| account.id == account_id)
            .ok_or_else(|| "账号不存在".to_string())?;
        account.account_name = normalize_optional(account_name.as_deref());
        let updated = account.clone();
        self.write_database(&database)?;
        Ok(updated)
    }

    pub fn update_api_key_bound_oauth_account(
        &self,
        account_id: &str,
        bound_oauth_account_id: Option<String>,
        bound_oauth_use_local_gateway: bool,
    ) -> Result<CodexAccount, String> {
        let mut database = self.read_database()?;
        let bound_id = normalize_optional(bound_oauth_account_id.as_deref());

        if let Some(bound_id) = bound_id.as_deref() {
            let oauth_account = database
                .accounts
                .iter()
                .find(|account| account.id == bound_id)
                .ok_or_else(|| "绑定的 OAuth 账号不存在".to_string())?;
            if oauth_account.auth_mode.as_deref() == Some("apikey") {
                return Err("API Key 只能绑定 OAuth 账号".to_string());
            }
            if oauth_account
                .tokens
                .refresh_token
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                return Err("绑定的 OAuth 账号缺少 refresh_token".to_string());
            }
        }

        let is_current = database.current_account_id.as_deref() == Some(account_id);
        let account = database
            .accounts
            .iter_mut()
            .find(|account| account.id == account_id)
            .ok_or_else(|| "账号不存在".to_string())?;
        if account.auth_mode.as_deref() != Some("apikey") {
            return Err("仅 API Key 账号支持绑定 OAuth 账号".to_string());
        }
        let binding_changed = account.bound_oauth_account_id != bound_id;
        account.bound_oauth_account_id = bound_id.clone();
        account.bound_oauth_use_local_gateway = bound_id.is_some() && bound_oauth_use_local_gateway;
        if binding_changed {
            clear_account_quota(account);
        }
        account.last_used = now_timestamp();
        let updated = account.clone();

        if is_current {
            write_codex_auth_projection(
                &self.codex_home,
                &updated,
                &database.accounts,
                ManagedModelTransition::default(),
            )?;
        }
        self.write_database(&database)?;
        Ok(updated)
    }

    pub fn update_account_phone(
        &self,
        account_id: &str,
        phone: String,
    ) -> Result<CodexAccount, String> {
        let mut database = self.read_database()?;
        let account = database
            .accounts
            .iter_mut()
            .find(|account| account.id == account_id)
            .ok_or_else(|| "账号不存在".to_string())?;
        account.bound_phone = normalize_optional(Some(&phone));
        let updated = account.clone();
        self.write_database(&database)?;
        Ok(updated)
    }

    pub fn update_account_from_json(
        &self,
        account_id: &str,
        json_content: &str,
    ) -> Result<CodexAccount, String> {
        let value: Value = serde_json::from_str(json_content)
            .map_err(|error| format!("JSON 解析失败: {}", error))?;
        let mut database = self.read_database()?;
        let old_account = database
            .accounts
            .iter()
            .find(|account| account.id == account_id)
            .cloned()
            .ok_or_else(|| "账号不存在".to_string())?;

        let import_value = select_account_value_for_update(&value, &old_account)?;
        let explicit_local_id = read_string(import_value, &["id"]);
        let mut updated = match serde_json::from_value::<CodexAccount>(import_value.clone()) {
            Ok(account) => account,
            Err(_) => self.account_from_import_value(import_value)?,
        };
        updated.id = explicit_local_id.unwrap_or_else(|| old_account.id.clone());
        if updated.created_at <= 0 {
            updated.created_at = old_account.created_at;
        }
        if updated.last_used <= 0 {
            updated.last_used = old_account.last_used;
        }
        apply_import_metadata(&mut updated, import_value);

        if updated.auth_mode.as_deref() == Some("apikey") {
            updated.default_model = updated
                .default_model
                .as_deref()
                .map(validate_default_model)
                .transpose()?;
        }
        if updated.id != old_account.id
            && database
                .accounts
                .iter()
                .any(|account| account.id == updated.id)
        {
            return Err(format!("账号 ID 已存在: {}", updated.id));
        }

        let was_current = database.current_account_id.as_deref() == Some(account_id);
        let refresh_current_bound_api = !was_current
            && database
                .current_account_id
                .as_deref()
                .and_then(|current_id| {
                    database
                        .accounts
                        .iter()
                        .find(|account| account.id == current_id)
                })
                .is_some_and(|account| {
                    account.auth_mode.as_deref() == Some("apikey")
                        && account.bound_oauth_account_id.as_deref()
                            == Some(old_account.id.as_str())
                });
        let mut model_transition = ManagedModelTransition::default();
        let mut target_default_model = None;
        if was_current {
            if database
                .managed_model_config_backup
                .as_ref()
                .is_some_and(|backup| {
                    backup.managed_account_id.as_deref() != Some(old_account.id.as_str())
                })
            {
                database.managed_model_config_backup = None;
            }
            let config_document = read_toml_document(&self.codex_home.join("config.toml"))?;
            let mut previous_default_model = (old_account.auth_mode.as_deref() == Some("apikey"))
                .then(|| {
                    old_account
                        .default_model
                        .as_deref()
                        .and_then(|model| normalize_optional(Some(model)))
                })
                .flatten();
            if previous_default_model.as_deref().is_some_and(|model| {
                config_document.get("model").and_then(Item::as_str) != Some(model)
            }) {
                database.managed_model_config_backup = None;
                previous_default_model = None;
            }
            target_default_model = (updated.auth_mode.as_deref() == Some("apikey"))
                .then(|| {
                    updated
                        .default_model
                        .as_deref()
                        .and_then(|model| normalize_optional(Some(model)))
                })
                .flatten();
            if target_default_model.is_some() && database.managed_model_config_backup.is_none() {
                database.managed_model_config_backup =
                    Some(capture_managed_model_config(&config_document, &updated.id));
            }
            if let Some(backup) = database.managed_model_config_backup.as_mut() {
                backup.managed_account_id = Some(updated.id.clone());
                if is_gpt_5_6_model(target_default_model.as_deref()) {
                    let provider_id = api_key_provider_id(&updated);
                    if !backup.touched_provider_ids.contains(&provider_id) {
                        backup.touched_provider_ids.push(provider_id);
                    }
                }
            }
            let leaving_managed_model = target_default_model.is_none()
                && (previous_default_model.is_some()
                    || database.managed_model_config_backup.is_some());
            let legacy_previous_provider_id = is_gpt_5_6_model(previous_default_model.as_deref())
                .then(|| api_key_provider_id(&old_account));
            let restore_provider_ids = database
                .managed_model_config_backup
                .as_ref()
                .map(|backup| backup.touched_provider_ids.clone())
                .filter(|ids| !ids.is_empty())
                .or_else(|| legacy_previous_provider_id.map(|id| vec![id]))
                .unwrap_or_default();
            model_transition = ManagedModelTransition {
                restore_model: leaving_managed_model,
                restore_gpt_5_6: leaving_managed_model
                    || (is_gpt_5_6_model(previous_default_model.as_deref())
                        && !is_gpt_5_6_model(target_default_model.as_deref())),
                expected_model: previous_default_model,
                restore_provider_ids,
                backup: database.managed_model_config_backup.clone(),
            };
        }

        if updated.id != old_account.id {
            for account in &mut database.accounts {
                if account.bound_oauth_account_id.as_deref() == Some(old_account.id.as_str()) {
                    account.bound_oauth_account_id = Some(updated.id.clone());
                }
            }
        }
        database.accounts.retain(|account| account.id != account_id);
        database.accounts.insert(0, updated.clone());
        if was_current {
            database.current_account_id = Some(updated.id.clone());
            write_codex_auth_projection(
                &self.codex_home,
                &updated,
                &database.accounts,
                model_transition,
            )?;
            if target_default_model.is_none() {
                database.managed_model_config_backup = None;
            }
        } else if refresh_current_bound_api {
            if let Some(current) = database
                .current_account_id
                .as_deref()
                .and_then(|current_id| {
                    database
                        .accounts
                        .iter()
                        .find(|account| account.id == current_id)
                })
                .cloned()
            {
                write_codex_auth_projection(
                    &self.codex_home,
                    &current,
                    &database.accounts,
                    ManagedModelTransition::default(),
                )?;
            }
        }
        self.write_database(&database)?;
        Ok(updated)
    }

    pub fn update_account_quota(
        &self,
        account_id: &str,
        quota: CodexQuota,
    ) -> Result<CodexAccount, String> {
        let mut database = self.read_database()?;
        let account = database
            .accounts
            .iter_mut()
            .find(|account| account.id == account_id)
            .ok_or_else(|| "账号不存在".to_string())?;
        account.quota = Some(quota);
        account.quota_error = None;
        account.usage_updated_at = Some(now_timestamp());
        let updated = account.clone();
        self.write_database(&database)?;
        Ok(updated)
    }

    pub fn update_account_quota_error(
        &self,
        account_id: &str,
        message: String,
    ) -> Result<CodexAccount, String> {
        let mut database = self.read_database()?;
        let account = database
            .accounts
            .iter_mut()
            .find(|account| account.id == account_id)
            .ok_or_else(|| "账号不存在".to_string())?;
        let code = quota_error_code(&message);
        if code.as_deref() == Some("token_expired") {
            account.quota = None;
        }
        account.quota_error = Some(CodexQuotaErrorInfo {
            code,
            message,
            timestamp: now_timestamp(),
        });
        account.usage_updated_at = Some(now_timestamp());
        let updated = account.clone();
        self.write_database(&database)?;
        Ok(updated)
    }

    pub fn export_accounts(
        &self,
        account_ids: &[String],
        format: Option<&str>,
    ) -> Result<String, String> {
        let database = self.read_database()?;
        let selected: Vec<CodexAccount> = account_ids
            .iter()
            .filter_map(|id| {
                database
                    .accounts
                    .iter()
                    .find(|account| account.id == *id)
                    .cloned()
            })
            .collect();
        let export_value = match format.unwrap_or("cockpit_tools") {
            "sub2api" => export_sub2api_accounts(&selected, &database.accounts),
            "cpa" => export_cpa_accounts(&selected, &database.accounts),
            _ => serde_json::json!({
                "app": "Codex Switcher",
                "format": "codex-switcher.accounts",
                "version": 1,
                "exported_at": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                "accounts": selected
            }),
        };
        serde_json::to_string_pretty(&export_value)
            .map_err(|error| format!("序列化失败: {}", error))
    }

    pub fn delete_account(&self, account_id: &str) -> Result<(), String> {
        let mut database = self.read_database()?;
        let removed_account = database
            .accounts
            .iter()
            .find(|account| account.id == account_id)
            .cloned()
            .ok_or_else(|| "账号不存在".to_string())?;
        let removed_was_current = database.current_account_id.as_deref() == Some(account_id);
        if removed_was_current
            && (removed_account.default_model.is_some()
                || database.managed_model_config_backup.is_some())
        {
            write_managed_model_transition_only(
                &self.codex_home,
                ManagedModelTransition {
                    restore_model: true,
                    restore_gpt_5_6: true,
                    expected_model: removed_account.default_model.clone(),
                    restore_provider_ids: database
                        .managed_model_config_backup
                        .as_ref()
                        .map(|backup| backup.touched_provider_ids.clone())
                        .filter(|ids| !ids.is_empty())
                        .unwrap_or_else(|| {
                            if is_gpt_5_6_model(removed_account.default_model.as_deref()) {
                                vec![api_key_provider_id(&removed_account)]
                            } else {
                                Vec::new()
                            }
                        }),
                    backup: database.managed_model_config_backup.clone(),
                },
            )?;
            database.managed_model_config_backup = None;
        }
        let before = database.accounts.len();
        database.accounts.retain(|account| account.id != account_id);
        debug_assert!(database.accounts.len() < before);
        let mut removed_bound_references = false;
        for account in &mut database.accounts {
            if account.bound_oauth_account_id.as_deref() == Some(account_id) {
                account.bound_oauth_account_id = None;
                account.bound_oauth_use_local_gateway = false;
                clear_account_quota(account);
                removed_bound_references = true;
            }
        }
        if removed_was_current {
            database.current_account_id = None;
        }
        if removed_bound_references {
            if let Some(current_id) = database.current_account_id.as_deref() {
                if let Some(current) = database
                    .accounts
                    .iter()
                    .find(|account| account.id == current_id)
                    .cloned()
                {
                    write_codex_auth_projection(
                        &self.codex_home,
                        &current,
                        &database.accounts,
                        ManagedModelTransition::default(),
                    )?;
                }
            }
        }
        self.write_database(&database)
    }

    pub fn switch_account(&self, account_id: &str) -> Result<CodexAccount, String> {
        let mut database = self.read_database()?;
        if database
            .managed_model_config_backup
            .as_ref()
            .and_then(|backup| backup.managed_account_id.as_deref())
            != database.current_account_id.as_deref()
        {
            database.managed_model_config_backup = None;
        }
        let previous_account_index = database
            .current_account_id
            .as_deref()
            .filter(|current_id| *current_id != account_id)
            .and_then(|current_id| {
                database
                    .accounts
                    .iter()
                    .position(|account| account.id == current_id)
            });
        let config_document = read_toml_document(&self.codex_home.join("config.toml"))?;
        let mut previous_default_model = previous_account_index
            .and_then(|index| {
                let account = &database.accounts[index];
                (account.auth_mode.as_deref() == Some("apikey"))
                    .then_some(account.default_model.as_deref())
                    .flatten()
            })
            .and_then(|model| normalize_optional(Some(model)));
        if previous_default_model
            .as_deref()
            .is_some_and(|model| config_document.get("model").and_then(Item::as_str) != Some(model))
        {
            if let Some(index) = previous_account_index {
                database.accounts[index].default_model = None;
            }
            database.managed_model_config_backup = None;
            previous_default_model = None;
        }
        let target_index = database
            .accounts
            .iter()
            .position(|account| account.id == account_id)
            .ok_or_else(|| "账号不存在".to_string())?;
        let target_default_model = (database.accounts[target_index].auth_mode.as_deref()
            == Some("apikey"))
        .then(|| database.accounts[target_index].default_model.as_deref())
        .flatten()
        .and_then(|model| normalize_optional(Some(model)));
        if target_default_model.is_some() && database.managed_model_config_backup.is_none() {
            database.managed_model_config_backup =
                Some(capture_managed_model_config(&config_document, account_id));
        }
        if let Some(backup) = database.managed_model_config_backup.as_mut() {
            backup.managed_account_id = Some(account_id.to_string());
            if is_gpt_5_6_model(target_default_model.as_deref()) {
                let provider_id = api_key_provider_id(&database.accounts[target_index]);
                if !backup.touched_provider_ids.contains(&provider_id) {
                    backup.touched_provider_ids.push(provider_id);
                }
            }
        }
        let leaving_managed_model = target_default_model.is_none()
            && (previous_default_model.is_some() || database.managed_model_config_backup.is_some());
        let legacy_previous_provider_id = previous_account_index
            .filter(|_| is_gpt_5_6_model(previous_default_model.as_deref()))
            .map(|index| api_key_provider_id(&database.accounts[index]));
        let restore_provider_ids = database
            .managed_model_config_backup
            .as_ref()
            .map(|backup| backup.touched_provider_ids.clone())
            .filter(|ids| !ids.is_empty())
            .or_else(|| legacy_previous_provider_id.map(|id| vec![id]))
            .unwrap_or_default();
        let model_transition = ManagedModelTransition {
            restore_model: leaving_managed_model,
            restore_gpt_5_6: leaving_managed_model
                || (is_gpt_5_6_model(previous_default_model.as_deref())
                    && !is_gpt_5_6_model(target_default_model.as_deref())),
            expected_model: previous_default_model,
            restore_provider_ids,
            backup: database.managed_model_config_backup.clone(),
        };
        database.accounts[target_index].last_used = now_timestamp();
        let switched = database.accounts[target_index].clone();

        write_codex_auth_projection(
            &self.codex_home,
            &switched,
            &database.accounts,
            model_transition,
        )?;
        if target_default_model.is_none() {
            database.managed_model_config_backup = None;
        }
        database.current_account_id = Some(switched.id.clone());
        self.write_database(&database)?;
        Ok(switched)
    }

    pub fn restart_codex_app(&self) -> Result<String, String> {
        restart_codex_app()
    }

    fn database_path(&self) -> PathBuf {
        self.storage_dir.join("accounts.json")
    }

    fn read_database(&self) -> Result<AccountDatabase, String> {
        let path = self.database_path();
        if !path.exists() {
            return Ok(AccountDatabase::default());
        }
        let content =
            fs::read_to_string(&path).map_err(|error| format!("读取账号库失败: {}", error))?;
        serde_json::from_str(&content).map_err(|error| format!("解析账号库失败: {}", error))
    }

    fn write_database(&self, database: &AccountDatabase) -> Result<(), String> {
        fs::create_dir_all(&self.storage_dir)
            .map_err(|error| format!("创建账号目录失败: {}", error))?;
        let content = serde_json::to_string_pretty(database)
            .map_err(|error| format!("序列化账号库失败: {}", error))?;
        write_string_atomic(&self.database_path(), &content)
    }

    fn read_current_codex_config(&self) -> Result<CodexCurrentConfig, String> {
        let mut config = CodexCurrentConfig::default();
        let auth_path = self.codex_home.join("auth.json");
        if auth_path.exists() {
            let content = fs::read_to_string(&auth_path)
                .map_err(|error| format!("读取 auth.json 失败: {}", error))?;
            let auth: Value = serde_json::from_str(&content)
                .map_err(|error| format!("解析 auth.json 失败: {}", error))?;
            config.api_key = read_string(
                &auth,
                &["OPENAI_API_KEY", "openai_api_key", "apiKey", "api_key"],
            );
            let token_source = auth.get("tokens").unwrap_or(&auth);
            config.oauth_access_token = read_string(token_source, &["access_token", "accessToken"]);
            let id_token = read_string(token_source, &["id_token", "idToken"]).unwrap_or_default();
            config.oauth_email = jwt_claim_string(&id_token, "email")
                .or_else(|| {
                    config
                        .oauth_access_token
                        .as_deref()
                        .and_then(|token| jwt_claim_string(token, "email"))
                })
                .or_else(|| read_string(&auth, &["email"]));
        }

        let config_path = self.codex_home.join("config.toml");
        if config_path.exists() {
            let document = read_toml_document(&config_path)?;
            if let Some(provider) = document["model_provider"].as_str().and_then(|provider_id| {
                document
                    .get("model_providers")
                    .and_then(|providers| providers.get(provider_id))
            }) {
                config.provider_base_url = provider
                    .get("base_url")
                    .and_then(|value| value.as_str())
                    .and_then(|value| normalize_optional(Some(value)));
                config.provider_api_key = provider
                    .get("experimental_bearer_token")
                    .and_then(|value| value.as_str())
                    .and_then(|value| normalize_optional(Some(value)));
            }
        }
        Ok(config)
    }

    fn account_from_import_value(&self, value: &Value) -> Result<CodexAccount, String> {
        if let Some(api_key) = read_string(
            value,
            &["OPENAI_API_KEY", "openai_api_key", "apiKey", "api_key"],
        ) {
            let mut account = build_api_key_account_record(
                api_key,
                read_string(value, &["base_url", "api_base_url", "apiBaseUrl"]),
                read_string(value, &["api_provider_name", "providerName", "provider"]),
                read_string(
                    value,
                    &[
                        "api_official_url",
                        "apiOfficialUrl",
                        "official_url",
                        "officialUrl",
                        "website",
                        "homepage",
                    ],
                ),
                read_string(value, &["account_name", "name", "label"]),
            )?;
            apply_import_metadata(&mut account, value);
            return Ok(account);
        }

        let token_source = value.get("tokens").unwrap_or(value);
        let access_token = read_string(token_source, &["access_token", "accessToken"])
            .or_else(|| match value {
                Value::String(raw) => Some(raw.trim().to_string()),
                _ => None,
            })
            .filter(|token| !token.is_empty())
            .ok_or_else(|| "JSON 中没有找到 access_token".to_string())?;
        let id_token = read_string(token_source, &["id_token", "idToken"]).unwrap_or_default();
        let refresh_token = read_string(token_source, &["refresh_token", "refreshToken"]);
        let email = read_string(value, &["email"])
            .or_else(|| {
                value
                    .get("user")
                    .and_then(|user| read_string(user, &["email"]))
            })
            .or_else(|| jwt_claim_string(&id_token, "email"))
            .or_else(|| jwt_claim_string(&access_token, "email"))
            .unwrap_or_else(|| format!("codex-{}@local", short_hash(&access_token, 8)));
        let now = now_timestamp();
        let mut account = CodexAccount {
            id: read_string(value, &["id", "account_id", "accountId"])
                .unwrap_or_else(|| build_oauth_account_id(&email, &access_token)),
            email,
            account_name: read_string(value, &["account_name", "name"]),
            auth_mode: None,
            openai_api_key: None,
            api_base_url: None,
            api_provider_name: None,
            api_official_url: None,
            default_model: None,
            plan_type: None,
            auth_file_plan_type: None,
            bound_oauth_account_id: None,
            bound_oauth_use_local_gateway: false,
            bound_phone: None,
            subscription_active_until: None,
            access_token_expires_at: None,
            quota: None,
            quota_error: None,
            usage_updated_at: None,
            tokens: CodexTokens {
                id_token,
                access_token,
                refresh_token,
            },
            created_at: read_i64(value, &["created_at", "createdAt"]).unwrap_or(now),
            last_used: read_i64(value, &["last_used", "lastUsed"]).unwrap_or(now),
        };
        apply_import_metadata(&mut account, value);
        Ok(account)
    }
}

pub fn codex_restart_commands() -> (
    &'static str,
    Vec<&'static str>,
    &'static str,
    Vec<&'static str>,
) {
    #[cfg(target_os = "macos")]
    {
        (
            "pkill",
            vec!["-x", "Codex"],
            "open",
            vec!["-n", "-a", "Codex"],
        )
    }

    #[cfg(target_os = "windows")]
    {
        (
            "taskkill",
            vec!["/IM", "Codex.exe", "/F"],
            "powershell",
            vec![
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                WINDOWS_CODEX_START_SCRIPT,
            ],
        )
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        ("pkill", vec!["-x", "codex"], "codex", vec![])
    }
}

#[cfg(target_os = "windows")]
const WINDOWS_CODEX_START_SCRIPT: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
function Start-CodexStoreTarget([string]$target) {
  if ([string]::IsNullOrWhiteSpace($target)) { return $false }
  try {
    Start-Process -FilePath $target -ErrorAction Stop
    return $true
  } catch {
    return $false
  }
}

function Start-CodexExe([string]$path) {
  if ([string]::IsNullOrWhiteSpace($path)) { return $false }
  if ($path -match '(?i)codex[-_\s]*switcher') { return $false }
  if ((Split-Path -Leaf $path) -ine 'Codex.exe') { return $false }
  if (Test-Path -LiteralPath $path) {
    try {
      Start-Process -FilePath $path -ErrorAction Stop
      return $true
    } catch {
      return $false
    }
  }
  return $false
}

$startApp = Get-StartApps | Where-Object { $_.AppID -like 'OpenAI.Codex_*' } | Select-Object -First 1
if ($startApp -and -not [string]::IsNullOrWhiteSpace($startApp.AppID)) {
  $target = 'shell:AppsFolder\' + [string]$startApp.AppID
  if (Start-CodexStoreTarget $target) { exit 0 }
}

$pkg = Get-AppxPackage -Name 'OpenAI.Codex' -ErrorAction SilentlyContinue |
  Sort-Object -Property Version -Descending |
  Select-Object -First 1
if ($pkg -and -not [string]::IsNullOrWhiteSpace($pkg.PackageFamilyName)) {
  $target = 'shell:AppsFolder\' + [string]($pkg.PackageFamilyName.Trim() + '!App')
  if (Start-CodexStoreTarget $target) { exit 0 }
}
if ($pkg -and -not [string]::IsNullOrWhiteSpace($pkg.InstallLocation)) {
  $appxExe = Join-Path ([string]$pkg.InstallLocation.Trim()) 'app\Codex.exe'
  if (Start-CodexExe $appxExe) { exit 0 }
}

$windowsAppsRoots = @()
$windowsAppsRoots += 'C:\Program Files\WindowsApps'
Get-PSDrive -PSProvider FileSystem | ForEach-Object {
  $root = $_.Root
  if ([string]::IsNullOrWhiteSpace($root)) { return }
  $windowsAppsRoots += (Join-Path $root 'WindowsApps')
}
foreach ($root in ($windowsAppsRoots | Select-Object -Unique)) {
  if (-not (Test-Path -LiteralPath $root)) { continue }
  $entries = Get-ChildItem -LiteralPath $root -Directory -Filter 'OpenAI.Codex_*' -ErrorAction SilentlyContinue |
    Sort-Object -Property Name -Descending
  foreach ($entry in $entries) {
    $exe = Join-Path $entry.FullName 'app\Codex.exe'
    if (Start-CodexExe $exe) { exit 0 }
  }
}

$candidates = @()
if ($env:LOCALAPPDATA) {
  $candidates += (Join-Path $env:LOCALAPPDATA 'Programs\Codex\Codex.exe')
  $candidates += (Join-Path $env:LOCALAPPDATA 'Codex\Codex.exe')
}
if ($env:ProgramFiles) {
  $candidates += (Join-Path $env:ProgramFiles 'Codex\Codex.exe')
}
${pf86} = ${env:ProgramFiles(x86)}
if (${pf86}) {
  $candidates += (Join-Path ${pf86} 'Codex\Codex.exe')
}
foreach ($path in $candidates) {
  if (Start-CodexExe $path) { exit 0 }
}

$shortcutRoots = @()
if ($env:APPDATA) {
  $shortcutRoots += (Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs')
}
if ($env:ProgramData) {
  $shortcutRoots += (Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs')
}
foreach ($root in $shortcutRoots) {
  if (-not (Test-Path -LiteralPath $root)) { continue }
  $shortcut = Get-ChildItem -LiteralPath $root -Filter '*.lnk' -Recurse -ErrorAction SilentlyContinue |
    Where-Object { $_.BaseName -match '^(Codex|OpenAI Codex)$' -and $_.FullName -notmatch '(?i)codex[-_\s]*switcher' } |
    Select-Object -First 1
  if ($shortcut) {
    Start-Process -FilePath $shortcut.FullName
    exit 0
  }
}

foreach ($commandName in @('Codex.exe', 'codex.exe')) {
  $command = Get-Command $commandName -CommandType Application -ErrorAction SilentlyContinue
  if ($command -and (Start-CodexExe $command.Source)) { exit 0 }
}
exit 1
"#;

pub fn codex_restart_delay_ms() -> u64 {
    #[cfg(target_os = "macos")]
    {
        1_200
    }

    #[cfg(not(target_os = "macos"))]
    {
        300
    }
}

fn restart_codex_app() -> Result<String, String> {
    let (stop_program, stop_args, start_program, start_args) = codex_restart_commands();
    let mut stop_command = Command::new(stop_program);
    stop_command.args(stop_args);
    hide_command_window(&mut stop_command);
    let _ = stop_command.status();
    thread::sleep(Duration::from_millis(codex_restart_delay_ms()));
    let mut start_command = Command::new(start_program);
    start_command.args(start_args);
    hide_command_window(&mut start_command);
    let status = start_command
        .status()
        .map_err(|error| format!("启动 Codex 失败: {}", error))?;
    if !status.success() {
        return Err(
            "启动 Codex 失败：未找到 Codex 桌面应用，请确认已安装并可从开始菜单打开".to_string(),
        );
    }
    Ok("已尝试重启 Codex".to_string())
}

#[cfg(windows)]
fn hide_command_window(command: &mut Command) {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_command_window(_command: &mut Command) {}

fn upsert_account(accounts: &mut Vec<CodexAccount>, account: CodexAccount) {
    if let Some(existing) = accounts.iter_mut().find(|item| item.id == account.id) {
        let created_at = existing.created_at;
        *existing = account;
        existing.created_at = created_at;
    } else {
        accounts.insert(0, account);
    }
}

fn now_timestamp() -> i64 {
    chrono::Utc::now().timestamp()
}

fn quota_error_code(message: &str) -> Option<String> {
    let lower = message.to_ascii_lowercase();
    if lower.contains("token_expired")
        || lower.contains("unauthorized")
        || lower.contains("401")
        || lower.contains("authentication token is expired")
    {
        return Some("token_expired".to_string());
    }
    None
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_base_url(value: Option<&str>) -> Option<String> {
    normalize_optional(value).map(|url| url.trim_end_matches('/').to_ascii_lowercase())
}

fn account_base_url(account: &CodexAccount) -> Option<String> {
    normalize_base_url(account.api_base_url.as_deref())
        .or_else(|| Some("https://api.openai.com/v1".to_string()))
}

fn oauth_identity_matches(account: &CodexAccount, config: &CodexCurrentConfig) -> bool {
    let email_matches = config
        .oauth_email
        .as_deref()
        .is_some_and(|email| account.email.eq_ignore_ascii_case(email));
    let token_matches = config
        .oauth_access_token
        .as_deref()
        .is_some_and(|token| account.tokens.access_token == token);
    email_matches || token_matches
}

fn strict_api_key_model_access_matches(
    accounts: &[CodexAccount],
    account: &CodexAccount,
    config: &CodexCurrentConfig,
) -> bool {
    if account.auth_mode.as_deref() != Some("apikey") {
        return false;
    }
    let Some(account_api_key) = account
        .openai_api_key
        .as_deref()
        .and_then(|value| normalize_optional(Some(value)))
    else {
        return false;
    };
    if config.provider_api_key.as_deref() != Some(account_api_key.as_str())
        || normalize_base_url(config.provider_base_url.as_deref()) != account_base_url(account)
        || config
            .api_key
            .as_deref()
            .is_some_and(|auth_api_key| auth_api_key != account_api_key.as_str())
    {
        return false;
    }

    if let Some(bound_oauth_id) = account.bound_oauth_account_id.as_deref() {
        return accounts
            .iter()
            .find(|candidate| {
                candidate.id == bound_oauth_id && candidate.auth_mode.as_deref() != Some("apikey")
            })
            .is_some_and(|oauth| oauth_identity_matches(oauth, config));
    }

    config.api_key.as_deref() == Some(account_api_key.as_str())
}

fn match_current_account_from_config(
    accounts: &[CodexAccount],
    config: &CodexCurrentConfig,
) -> Option<CodexAccount> {
    let provider_base_url = normalize_base_url(config.provider_base_url.as_deref());
    let api_key = config
        .provider_api_key
        .as_deref()
        .or(config.api_key.as_deref());

    if provider_base_url.is_some() || api_key.is_some() {
        let mut scored: Vec<(i32, CodexAccount)> = accounts
            .iter()
            .filter(|account| account.auth_mode.as_deref() == Some("apikey"))
            .filter_map(|account| {
                let base_matches = provider_base_url
                    .as_deref()
                    .is_some_and(|base_url| account_base_url(account).as_deref() == Some(base_url));
                let key_matches = api_key.is_some_and(|key| {
                    account
                        .openai_api_key
                        .as_deref()
                        .is_some_and(|account_key| account_key == key)
                });
                let mut score = 0;
                if base_matches {
                    score += 10;
                }
                if key_matches {
                    score += 6;
                }
                if let Some(bound_oauth_id) = account.bound_oauth_account_id.as_deref() {
                    let bound_matches = accounts
                        .iter()
                        .find(|candidate| candidate.id == bound_oauth_id)
                        .is_some_and(|oauth| oauth_identity_matches(oauth, config));
                    if !base_matches || !bound_matches {
                        return None;
                    }
                    score += 8;
                } else if !key_matches {
                    return None;
                }
                Some((score, account.clone()))
            })
            .collect();
        scored.sort_by(|left, right| right.0.cmp(&left.0));
        if let Some((_, account)) = scored.into_iter().next() {
            return Some(account);
        }
    }

    accounts
        .iter()
        .find(|account| {
            account.auth_mode.as_deref() != Some("apikey")
                && oauth_identity_matches(account, config)
        })
        .cloned()
}

fn validate_api_key_credentials(
    api_key: &str,
    api_base_url: Option<&str>,
) -> Result<(String, Option<String>), String> {
    let api_key =
        normalize_optional(Some(api_key)).ok_or_else(|| "API Key 不能为空".to_string())?;
    if looks_like_url(&api_key) {
        return Err("API Key 不能是 URL，请检查是否填反".to_string());
    }
    let api_base_url =
        normalize_optional(api_base_url).map(|url| url.trim_end_matches('/').to_string());
    if let Some(base_url) = api_base_url.as_deref() {
        if !looks_like_url(base_url) {
            return Err("Base URL 必须以 http:// 或 https:// 开头".to_string());
        }
        if base_url.eq_ignore_ascii_case(&api_key) {
            return Err("API Key 不能与 Base URL 相同".to_string());
        }
    }
    Ok((api_key, api_base_url))
}

fn build_api_key_account_record(
    api_key: String,
    api_base_url: Option<String>,
    api_provider_name: Option<String>,
    api_official_url: Option<String>,
    account_name: Option<String>,
) -> Result<CodexAccount, String> {
    let (api_key, api_base_url) = validate_api_key_credentials(&api_key, api_base_url.as_deref())?;
    let api_official_url = validate_optional_url(api_official_url.as_deref(), "官网地址")?;
    let now = now_timestamp();
    Ok(CodexAccount {
        id: build_api_key_account_id(&api_key),
        email: build_api_key_email(&api_key),
        account_name: normalize_optional(account_name.as_deref())
            .or_else(|| normalize_optional(api_provider_name.as_deref())),
        auth_mode: Some("apikey".to_string()),
        openai_api_key: Some(api_key),
        api_base_url,
        api_provider_name: normalize_optional(api_provider_name.as_deref()),
        api_official_url,
        default_model: None,
        plan_type: Some("api_key".to_string()),
        auth_file_plan_type: None,
        bound_oauth_account_id: None,
        bound_oauth_use_local_gateway: false,
        bound_phone: None,
        subscription_active_until: None,
        access_token_expires_at: None,
        quota: None,
        quota_error: None,
        usage_updated_at: None,
        tokens: CodexTokens {
            id_token: String::new(),
            access_token: String::new(),
            refresh_token: None,
        },
        created_at: now,
        last_used: now,
    })
}

fn validate_default_model(model_id: &str) -> Result<String, String> {
    let model_id =
        normalize_optional(Some(model_id)).ok_or_else(|| "默认模型不能为空".to_string())?;
    if model_id.chars().count() > 256 {
        return Err("默认模型名称不能超过 256 个字符".to_string());
    }
    if model_id.chars().any(char::is_control) {
        return Err("默认模型名称不能包含控制字符".to_string());
    }
    Ok(model_id)
}

fn validate_optional_url(value: Option<&str>, label: &str) -> Result<Option<String>, String> {
    let Some(url) = normalize_optional(value).map(|item| item.trim_end_matches('/').to_string())
    else {
        return Ok(None);
    };
    if !looks_like_url(&url) {
        return Err(format!("{}必须以 http:// 或 https:// 开头", label));
    }
    Ok(Some(url))
}

fn looks_like_url(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

fn build_api_key_account_id(api_key: &str) -> String {
    format!("api_{}", short_hash(api_key, 16))
}

fn build_oauth_account_id(email: &str, access_token: &str) -> String {
    format!(
        "codex_{}",
        short_hash(&format!("{}:{}", email, access_token), 16)
    )
}

fn build_api_key_email(api_key: &str) -> String {
    let trimmed = api_key.trim();
    let suffix = if trimmed.len() <= 6 {
        trimmed
    } else {
        &trimmed[trimmed.len() - 6..]
    };
    format!("api***{}", suffix)
}

fn short_hash(value: &str, len: usize) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let hex = digest
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect::<String>();
    hex.chars().take(len).collect()
}

fn read_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .and_then(|item| normalize_optional(Some(item)))
}

fn read_i64(value: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|item| {
            item.as_i64()
                .or_else(|| item.as_u64().and_then(|raw| i64::try_from(raw).ok()))
        })
    })
}

fn apply_import_metadata(account: &mut CodexAccount, value: &Value) {
    account.plan_type = read_string(
        value,
        &[
            "plan_type",
            "planType",
            "chatgpt_plan_type",
            "chatgptPlanType",
            "subscription_tier",
            "subscriptionTier",
        ],
    )
    .or_else(|| jwt_auth_claim_string(&account.tokens.id_token, "chatgpt_plan_type"))
    .or_else(|| jwt_auth_claim_string(&account.tokens.access_token, "chatgpt_plan_type"))
    .or_else(|| account.plan_type.clone());
    account.auth_file_plan_type = read_string(value, &["auth_file_plan_type", "authFilePlanType"])
        .or_else(|| account.auth_file_plan_type.clone());
    account.bound_oauth_account_id =
        read_string(value, &["bound_oauth_account_id", "boundOauthAccountId"])
            .or_else(|| account.bound_oauth_account_id.clone());
    account.bound_oauth_use_local_gateway = read_bool(
        value,
        &["bound_oauth_use_local_gateway", "boundOauthUseLocalGateway"],
    )
    .unwrap_or(account.bound_oauth_use_local_gateway);
    account.bound_phone = read_string(value, &["bound_phone", "boundPhone", "phone"])
        .or_else(|| account.bound_phone.clone());
    account.api_official_url = read_string(
        value,
        &[
            "api_official_url",
            "apiOfficialUrl",
            "official_url",
            "officialUrl",
            "website",
            "homepage",
        ],
    )
    .or_else(|| account.api_official_url.clone());
    account.default_model = read_string(value, &["default_model", "defaultModel", "model"])
        .or_else(|| account.default_model.clone());
    account.subscription_active_until = read_scalar_string(
        value,
        &[
            "subscription_active_until",
            "subscriptionActiveUntil",
            "chatgpt_subscription_active_until",
            "active_until",
            "activeUntil",
            "valid_until",
            "validUntil",
        ],
    )
    .or_else(|| {
        jwt_auth_claim_string(
            &account.tokens.id_token,
            "chatgpt_subscription_active_until",
        )
    })
    .or_else(|| account.subscription_active_until.clone());
    account.access_token_expires_at = read_scalar_string(
        value,
        &[
            "access_token_expires_at",
            "accessTokenExpiresAt",
            "token_expires_at",
            "tokenExpiresAt",
            "expires_at",
            "expiresAt",
            "expired",
        ],
    )
    .or_else(|| jwt_claim_string(&account.tokens.access_token, "exp"))
    .or_else(|| jwt_claim_string(&account.tokens.id_token, "exp"))
    .or_else(|| account.access_token_expires_at.clone());
    if let Some(quota_value) = value.get("quota") {
        if let Ok(quota) = serde_json::from_value::<CodexQuota>(quota_value.clone()) {
            account.quota = Some(quota);
        }
    }
    if let Some(error_value) = value.get("quota_error") {
        if let Ok(error) = serde_json::from_value::<CodexQuotaErrorInfo>(error_value.clone()) {
            account.quota_error = Some(error);
        }
    }
    account.usage_updated_at =
        read_i64(value, &["usage_updated_at", "usageUpdatedAt"]).or(account.usage_updated_at);
}

fn export_sub2api_accounts(selected: &[CodexAccount], accounts: &[CodexAccount]) -> Value {
    let exported_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    serde_json::json!({
        "exported_at": exported_at,
        "proxies": [],
        "accounts": selected
            .iter()
            .map(|account| {
                let export_account = token_export_account(account, accounts);
                serde_json::json!({
                    "name": export_account.account_name.clone().unwrap_or_else(|| export_account.email.clone()),
                    "platform": "openai",
                    "type": "oauth",
                    "credentials": build_sub2api_credentials(&export_account),
                    "concurrency": 0,
                    "priority": 0
                })
            })
            .collect::<Vec<Value>>(),
        "type": "sub2api-data",
        "version": 1
    })
}

fn export_cpa_accounts(selected: &[CodexAccount], accounts: &[CodexAccount]) -> Value {
    let payload = selected
        .iter()
        .map(|account| portable_token_storage(&token_export_account(account, accounts)))
        .collect::<Vec<Value>>();
    if payload.len() == 1 {
        payload.into_iter().next().unwrap_or(Value::Null)
    } else {
        Value::Array(payload)
    }
}

fn token_export_account(account: &CodexAccount, accounts: &[CodexAccount]) -> CodexAccount {
    if account.auth_mode.as_deref() != Some("apikey")
        || !account.tokens.access_token.trim().is_empty()
    {
        return account.clone();
    }

    let mut source = account
        .bound_oauth_account_id
        .as_deref()
        .and_then(|id| accounts.iter().find(|candidate| candidate.id == id))
        .cloned()
        .unwrap_or_else(|| account.clone());
    source.account_name = account.account_name.clone().or(source.account_name);
    source.bound_phone = account.bound_phone.clone().or(source.bound_phone);
    source.default_model = account.default_model.clone().or(source.default_model);
    source
}

fn build_sub2api_credentials(account: &CodexAccount) -> Value {
    let mut credentials = serde_json::Map::new();
    credentials.insert(
        "access_token".to_string(),
        Value::String(account.tokens.access_token.clone()),
    );
    insert_optional(
        &mut credentials,
        "expires_at",
        resolve_access_token_expiry(account),
    );
    if let Some(refresh_token) = account
        .tokens
        .refresh_token
        .as_deref()
        .and_then(|value| normalize_optional(Some(value)))
    {
        credentials.insert("refresh_token".to_string(), Value::String(refresh_token));
    }
    if let Some(id_token) = normalize_optional(Some(&account.tokens.id_token)) {
        credentials.insert("id_token".to_string(), Value::String(id_token));
    }
    insert_optional(
        &mut credentials,
        "email",
        normalize_optional(Some(&account.email)),
    );
    insert_account_metadata(&mut credentials, account);
    insert_optional(
        &mut credentials,
        "chatgpt_account_id",
        resolve_auth_field(account, "chatgpt_account_id")
            .or_else(|| resolve_auth_field(account, "account_id")),
    );
    insert_optional(
        &mut credentials,
        "chatgpt_user_id",
        resolve_auth_field(account, "chatgpt_user_id")
            .or_else(|| resolve_auth_field(account, "user_id"))
            .or_else(|| jwt_claim_string(&account.tokens.id_token, "sub")),
    );
    insert_optional(
        &mut credentials,
        "organization_id",
        resolve_auth_field(account, "organization_id"),
    );
    insert_optional(
        &mut credentials,
        "plan_type",
        resolve_auth_field(account, "chatgpt_plan_type"),
    );
    insert_optional(
        &mut credentials,
        "subscription_expires_at",
        resolve_subscription_expiry(account),
    );
    Value::Object(credentials)
}

fn portable_token_storage(account: &CodexAccount) -> Value {
    let mut payload = serde_json::Map::new();
    payload.insert(
        "id_token".to_string(),
        Value::String(account.tokens.id_token.clone()),
    );
    payload.insert(
        "access_token".to_string(),
        Value::String(account.tokens.access_token.clone()),
    );
    payload.insert(
        "refresh_token".to_string(),
        Value::String(account.tokens.refresh_token.clone().unwrap_or_default()),
    );
    payload.insert(
        "account_id".to_string(),
        Value::String(
            resolve_auth_field(account, "chatgpt_account_id")
                .or_else(|| resolve_auth_field(account, "account_id"))
                .unwrap_or_default(),
        ),
    );
    payload.insert(
        "last_refresh".to_string(),
        Value::String(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true)),
    );
    payload.insert("email".to_string(), Value::String(account.email.clone()));
    payload.insert("type".to_string(), Value::String("codex".to_string()));
    payload.insert(
        "expired".to_string(),
        Value::String(resolve_access_token_expiry(account).unwrap_or_default()),
    );
    insert_account_metadata(&mut payload, account);
    Value::Object(payload)
}

fn insert_account_metadata(payload: &mut serde_json::Map<String, Value>, account: &CodexAccount) {
    if let Some(provider) = account
        .api_provider_name
        .as_deref()
        .and_then(|value| normalize_optional(Some(value)))
    {
        payload.insert("api_provider_name".to_string(), Value::String(provider));
    }
    if let Some(base_url) = account
        .api_base_url
        .as_deref()
        .and_then(|value| normalize_optional(Some(value)))
    {
        payload.insert("api_base_url".to_string(), Value::String(base_url));
    }
    if let Some(official_url) = account
        .api_official_url
        .as_deref()
        .and_then(|value| normalize_optional(Some(value)))
    {
        payload.insert("api_official_url".to_string(), Value::String(official_url));
    }
    if let Some(default_model) = account
        .default_model
        .as_deref()
        .and_then(|value| normalize_optional(Some(value)))
    {
        payload.insert("default_model".to_string(), Value::String(default_model));
    }
    if let Some(phone) = account
        .bound_phone
        .as_deref()
        .and_then(|value| normalize_optional(Some(value)))
    {
        payload.insert("bound_phone".to_string(), Value::String(phone.clone()));
        payload.insert("phone".to_string(), Value::String(phone));
    }
}

fn insert_optional(payload: &mut serde_json::Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        payload.insert(key.to_string(), Value::String(value));
    }
}

fn resolve_subscription_expiry(account: &CodexAccount) -> Option<String> {
    normalize_timestamp_to_iso(account.subscription_active_until.as_deref()).or_else(|| {
        jwt_auth_claim_string(
            &account.tokens.id_token,
            "chatgpt_subscription_active_until",
        )
        .and_then(|value| normalize_timestamp_to_iso(Some(&value)))
    })
}

fn resolve_access_token_expiry(account: &CodexAccount) -> Option<String> {
    normalize_timestamp_to_iso(account.access_token_expires_at.as_deref())
        .or_else(|| {
            jwt_claim_string(&account.tokens.access_token, "exp")
                .and_then(|value| normalize_timestamp_to_iso(Some(&value)))
        })
        .or_else(|| {
            jwt_claim_string(&account.tokens.id_token, "exp")
                .and_then(|value| normalize_timestamp_to_iso(Some(&value)))
        })
}

fn normalize_timestamp_to_iso(value: Option<&str>) -> Option<String> {
    let trimmed = normalize_optional(value)?;
    if let Ok(number) = trimmed.parse::<f64>() {
        let millis = if number > 1_000_000_000_000.0 {
            number as i64
        } else {
            (number * 1000.0) as i64
        };
        let date = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(millis)?;
        return Some(date.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    }
    if let Ok(date) = chrono::DateTime::parse_from_rfc3339(&trimmed) {
        return Some(
            date.with_timezone(&chrono::Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        );
    }
    Some(trimmed)
}

fn read_bool(value: &Value, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_bool))
}

fn read_scalar_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|item| match item {
            Value::String(raw) => normalize_optional(Some(raw)),
            Value::Number(number) => Some(number.to_string()),
            Value::Bool(raw) => Some(raw.to_string()),
            _ => None,
        })
    })
}

fn jwt_claim_string(token: &str, claim: &str) -> Option<String> {
    let value = jwt_payload_value(token)?;
    read_scalar_string(&value, &[claim])
}

fn jwt_auth_claim_string(token: &str, claim: &str) -> Option<String> {
    let value = jwt_payload_value(token)?;
    let auth = value.get("https://api.openai.com/auth")?;
    read_scalar_string(auth, &[claim])
}

fn resolve_auth_field(account: &CodexAccount, field: &str) -> Option<String> {
    jwt_auth_claim_string(&account.tokens.id_token, field)
        .or_else(|| jwt_auth_claim_string(&account.tokens.access_token, field))
}

fn jwt_payload_value(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let decoded = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice(&decoded).ok()
}

const MANAGED_MODEL_ROOT_KEYS: [&str; 8] = [
    "model",
    "model_reasoning_effort",
    "disable_response_storage",
    "network_access",
    "windows_wsl_setup_acknowledged",
    "requires_openai_auth",
    "supports_websockets",
    "websocket_v2",
];
const MANAGED_GPT_5_6_ROOT_KEYS: [&str; 7] = [
    "model_reasoning_effort",
    "disable_response_storage",
    "network_access",
    "windows_wsl_setup_acknowledged",
    "requires_openai_auth",
    "supports_websockets",
    "websocket_v2",
];
const MANAGED_GPT_5_6_FEATURE_KEYS: [&str; 5] = [
    "goals",
    "js_repl",
    "memories",
    "websocket_v2",
    "supports_websockets",
];
const MANAGED_GPT_5_6_PROVIDER_KEYS: [&str; 2] = ["supports_websockets", "websocket_v2"];

#[derive(Debug, Clone, Default)]
struct ManagedModelTransition {
    restore_model: bool,
    restore_gpt_5_6: bool,
    expected_model: Option<String>,
    restore_provider_ids: Vec<String>,
    backup: Option<ManagedModelConfigBackup>,
}

fn capture_managed_model_config(
    document: &Document,
    managed_account_id: &str,
) -> ManagedModelConfigBackup {
    let root_items = MANAGED_MODEL_ROOT_KEYS
        .iter()
        .filter_map(|key| {
            document
                .get(key)
                .map(|item| ((*key).to_string(), item.to_string()))
        })
        .collect();
    let features = document.get("features").and_then(Item::as_table_like);
    let feature_items = MANAGED_GPT_5_6_FEATURE_KEYS
        .iter()
        .filter_map(|key| {
            features
                .and_then(|table| table.get(key))
                .map(|item| ((*key).to_string(), item.to_string()))
        })
        .collect();
    let mut provider_ids = Vec::new();
    let mut provider_items = BTreeMap::new();
    if let Some(providers) = document
        .get("model_providers")
        .and_then(Item::as_table_like)
    {
        for (provider_id, provider) in providers.iter() {
            let Some(provider) = provider.as_table_like() else {
                continue;
            };
            provider_ids.push(provider_id.to_string());
            let items = MANAGED_GPT_5_6_PROVIDER_KEYS
                .iter()
                .filter_map(|key| {
                    provider
                        .get(key)
                        .map(|item| ((*key).to_string(), item.to_string()))
                })
                .collect::<BTreeMap<_, _>>();
            if !items.is_empty() {
                provider_items.insert(provider_id.to_string(), items);
            }
        }
    }
    ManagedModelConfigBackup {
        managed_account_id: Some(managed_account_id.to_string()),
        root_items,
        features_table_existed: features.is_some(),
        feature_items,
        provider_ids,
        provider_items,
        touched_provider_ids: Vec::new(),
    }
}

fn parse_managed_backup_item(raw: &str) -> Result<Item, String> {
    let mut document = format!("__codex_switcher_value = {}\n", raw)
        .parse::<Document>()
        .map_err(|error| format!("恢复 Codex 模型配置失败: {}", error))?;
    document
        .as_table_mut()
        .remove("__codex_switcher_value")
        .ok_or_else(|| "恢复 Codex 模型配置失败: 备份项为空".to_string())
}

fn restore_table_item(
    table: &mut dyn toml_edit::TableLike,
    key: &str,
    backup_items: &BTreeMap<String, String>,
) -> Result<(), String> {
    if let Some(raw) = backup_items.get(key) {
        table.insert(key, parse_managed_backup_item(raw)?);
    } else {
        table.remove(key);
    }
    Ok(())
}

fn write_codex_auth_projection(
    codex_home: &Path,
    account: &CodexAccount,
    accounts: &[CodexAccount],
    model_transition: ManagedModelTransition,
) -> Result<(), String> {
    fs::create_dir_all(codex_home).map_err(|error| format!("创建 Codex 目录失败: {}", error))?;
    let auth_value = if account.auth_mode.as_deref() == Some("apikey") {
        let api_key = account
            .openai_api_key
            .as_deref()
            .and_then(|value| normalize_optional(Some(value)))
            .ok_or_else(|| "API Key 账号缺少 OPENAI_API_KEY".to_string())?;
        if let Some(bound_oauth_id) = account
            .bound_oauth_account_id
            .as_deref()
            .and_then(|value| normalize_optional(Some(value)))
        {
            let oauth_account = accounts
                .iter()
                .find(|candidate| candidate.id == bound_oauth_id)
                .ok_or_else(|| "绑定的 OAuth 账号不存在".to_string())?;
            if oauth_account.tokens.access_token.trim().is_empty() {
                return Err("绑定的 OAuth 账号缺少 access_token".to_string());
            }
            serde_json::json!({
                "OPENAI_API_KEY": Value::Null,
                "tokens": {
                    "id_token": oauth_account.tokens.id_token,
                    "access_token": oauth_account.tokens.access_token,
                    "refresh_token": oauth_account.tokens.refresh_token.clone().unwrap_or_default()
                },
                "last_refresh": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()
            })
        } else {
            serde_json::json!({
                "auth_mode": "apikey",
                "OPENAI_API_KEY": api_key
            })
        }
    } else {
        if account.tokens.access_token.trim().is_empty() {
            return Err("OAuth 账号缺少 access_token，无法写入 auth.json".to_string());
        }
        serde_json::json!({
            "OPENAI_API_KEY": Value::Null,
            "tokens": {
                "id_token": account.tokens.id_token,
                "access_token": account.tokens.access_token,
                "refresh_token": account.tokens.refresh_token.clone().unwrap_or_default()
            },
            "last_refresh": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()
        })
    };
    write_string_atomic(
        &codex_home.join("auth.json"),
        &serde_json::to_string_pretty(&auth_value)
            .map_err(|error| format!("序列化 auth.json 失败: {}", error))?,
    )?;

    if account.auth_mode.as_deref() == Some("apikey") {
        write_api_key_provider_config(codex_home, account, model_transition)?;
    } else {
        write_official_provider_config(codex_home, model_transition)?;
    }
    Ok(())
}

fn write_api_key_provider_config(
    codex_home: &Path,
    account: &CodexAccount,
    model_transition: ManagedModelTransition,
) -> Result<(), String> {
    let api_key = account
        .openai_api_key
        .as_deref()
        .and_then(|value| normalize_optional(Some(value)))
        .ok_or_else(|| "API Key 账号缺少 OPENAI_API_KEY".to_string())?;
    let provider_name = account
        .api_provider_name
        .as_deref()
        .and_then(|value| normalize_optional(Some(value)))
        .unwrap_or_else(|| "API Key".to_string());
    let provider_id = api_key_provider_id(account);
    let base_url = account
        .api_base_url
        .as_deref()
        .and_then(|value| normalize_optional(Some(value)))
        .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

    let config_path = codex_home.join("config.toml");
    let mut document = read_toml_document(&config_path)?;
    document["model_provider"] = value(provider_id.clone());
    let default_model = account
        .default_model
        .as_deref()
        .and_then(|model| normalize_optional(Some(model)));
    let uses_gpt_5_6 = is_gpt_5_6_model(default_model.as_deref());
    apply_managed_model_transition(&mut document, &model_transition)?;
    if let Some(model) = default_model {
        document["model"] = value(model);
    }
    if uses_gpt_5_6 {
        document["model_reasoning_effort"] = value("high");
        document["disable_response_storage"] = value(true);
        document["network_access"] = value("enabled");
        document["windows_wsl_setup_acknowledged"] = value(true);
        document["requires_openai_auth"] = value(true);
        document["features"]["goals"] = value(true);
        document["features"]["js_repl"] = value(false);
        document["features"]["memories"] = value(true);
        document.as_table_mut().remove("supports_websockets");
        document.as_table_mut().remove("websocket_v2");
        if let Some(features) = document["features"].as_table_like_mut() {
            features.remove("websocket_v2");
            features.remove("supports_websockets");
        }
    }
    let provider = &mut document["model_providers"][&provider_id];
    provider["name"] = value(provider_name);
    provider["base_url"] = value(base_url);
    provider["wire_api"] = value("responses");
    provider["requires_openai_auth"] = value(true);
    provider["experimental_bearer_token"] = value(api_key);
    if let Some(table) = provider.as_table_like_mut() {
        table.remove("env_key");
        if uses_gpt_5_6 {
            table.remove("supports_websockets");
            table.remove("websocket_v2");
        } else if table.get("supports_websockets").is_none() {
            table.insert("supports_websockets", value(false));
        }
    }
    write_string_atomic(&config_path, &document.to_string())
}

fn write_official_provider_config(
    codex_home: &Path,
    model_transition: ManagedModelTransition,
) -> Result<(), String> {
    let config_path = codex_home.join("config.toml");
    let mut document = read_toml_document(&config_path)?;
    apply_managed_model_transition(&mut document, &model_transition)?;
    document["model_provider"] = value("openai");
    write_string_atomic(&config_path, &document.to_string())
}

fn write_managed_model_transition_only(
    codex_home: &Path,
    model_transition: ManagedModelTransition,
) -> Result<(), String> {
    let config_path = codex_home.join("config.toml");
    let mut document = read_toml_document(&config_path)?;
    apply_managed_model_transition(&mut document, &model_transition)?;
    write_string_atomic(&config_path, &document.to_string())
}

fn apply_managed_model_transition(
    document: &mut Document,
    transition: &ManagedModelTransition,
) -> Result<(), String> {
    if !transition.restore_model && !transition.restore_gpt_5_6 {
        return Ok(());
    }
    if transition
        .expected_model
        .as_deref()
        .is_some_and(|expected| document.get("model").and_then(Item::as_str) != Some(expected))
    {
        return Ok(());
    }

    if let Some(backup) = transition.backup.as_ref() {
        if transition.restore_model {
            restore_table_item(document.as_table_mut(), "model", &backup.root_items)?;
        }
        if transition.restore_gpt_5_6 {
            for key in MANAGED_GPT_5_6_ROOT_KEYS {
                if managed_gpt_5_6_root_value_is_unchanged(document, key) {
                    restore_table_item(document.as_table_mut(), key, &backup.root_items)?;
                }
            }
            if let Some(features) = document
                .get_mut("features")
                .and_then(Item::as_table_like_mut)
            {
                for key in MANAGED_GPT_5_6_FEATURE_KEYS {
                    if managed_gpt_5_6_feature_value_is_unchanged(features, key) {
                        restore_table_item(features, key, &backup.feature_items)?;
                    }
                }
                if !backup.features_table_existed && features.is_empty() {
                    document.as_table_mut().remove("features");
                }
            }
            for provider_id in &transition.restore_provider_ids {
                let provider_existed = backup.provider_ids.iter().any(|id| id == provider_id);
                let provider_backup = backup.provider_items.get(provider_id);
                if let Some(provider) = document
                    .get_mut("model_providers")
                    .and_then(Item::as_table_like_mut)
                    .and_then(|providers| providers.get_mut(provider_id))
                    .and_then(Item::as_table_like_mut)
                {
                    if provider_existed || provider_backup.is_some() {
                        let empty = BTreeMap::new();
                        let items = provider_backup.unwrap_or(&empty);
                        for key in MANAGED_GPT_5_6_PROVIDER_KEYS {
                            if provider.get(key).is_none() {
                                restore_table_item(provider, key, items)?;
                            }
                        }
                    } else {
                        for key in MANAGED_GPT_5_6_PROVIDER_KEYS {
                            if provider.get(key).is_none() {
                                provider.remove(key);
                            }
                        }
                    }
                }
            }
        }
        return Ok(());
    }

    if transition.restore_model {
        document.as_table_mut().remove("model");
    }
    if transition.restore_gpt_5_6 {
        remove_legacy_managed_gpt_5_6_values(document, &transition.restore_provider_ids);
    }
    Ok(())
}

fn managed_gpt_5_6_root_value_is_unchanged(document: &Document, key: &str) -> bool {
    match key {
        "model_reasoning_effort" => document.get(key).and_then(Item::as_str) == Some("high"),
        "disable_response_storage" | "windows_wsl_setup_acknowledged" | "requires_openai_auth" => {
            document.get(key).and_then(Item::as_bool) == Some(true)
        }
        "network_access" => document.get(key).and_then(Item::as_str) == Some("enabled"),
        "supports_websockets" | "websocket_v2" => document.get(key).is_none(),
        _ => false,
    }
}

fn managed_gpt_5_6_feature_value_is_unchanged(
    features: &dyn toml_edit::TableLike,
    key: &str,
) -> bool {
    match key {
        "goals" | "memories" => features.get(key).and_then(Item::as_bool) == Some(true),
        "js_repl" => features.get(key).and_then(Item::as_bool) == Some(false),
        "supports_websockets" | "websocket_v2" => features.get(key).is_none(),
        _ => false,
    }
}

fn remove_legacy_managed_gpt_5_6_values(document: &mut Document, provider_ids: &[String]) {
    let root_values_match = [
        (
            "model_reasoning_effort",
            document["model_reasoning_effort"].as_str() == Some("high"),
        ),
        (
            "disable_response_storage",
            document["disable_response_storage"].as_bool() == Some(true),
        ),
        (
            "network_access",
            document["network_access"].as_str() == Some("enabled"),
        ),
        (
            "windows_wsl_setup_acknowledged",
            document["windows_wsl_setup_acknowledged"].as_bool() == Some(true),
        ),
        (
            "requires_openai_auth",
            document["requires_openai_auth"].as_bool() == Some(true),
        ),
    ];
    for (key, matches) in root_values_match {
        if matches {
            document.as_table_mut().remove(key);
        }
    }
    if let Some(features) = document
        .get_mut("features")
        .and_then(Item::as_table_like_mut)
    {
        for (key, matches) in [
            (
                "goals",
                features.get("goals").and_then(Item::as_bool) == Some(true),
            ),
            (
                "js_repl",
                features.get("js_repl").and_then(Item::as_bool) == Some(false),
            ),
            (
                "memories",
                features.get("memories").and_then(Item::as_bool) == Some(true),
            ),
        ] {
            if matches {
                features.remove(key);
            }
        }
    }
    for provider_id in provider_ids {
        if let Some(provider) = document
            .get_mut("model_providers")
            .and_then(Item::as_table_like_mut)
            .and_then(|providers| providers.get_mut(provider_id))
            .and_then(Item::as_table_like_mut)
        {
            provider.remove("websocket_v2");
        }
    }
}

fn is_gpt_5_6_model(model: Option<&str>) -> bool {
    model
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
        .is_some_and(|value| value == "gpt-5.6" || value.starts_with("gpt-5.6-"))
}

fn read_toml_document(path: &Path) -> Result<Document, String> {
    if !path.exists() {
        return Ok(Document::new());
    }
    fs::read_to_string(path)
        .map_err(|error| format!("读取 config.toml 失败: {}", error))?
        .parse::<Document>()
        .map_err(|error| format!("解析 config.toml 失败: {}", error))
}

fn sanitize_provider_id(raw: &str) -> Option<String> {
    let mut normalized = String::new();
    let mut previous_separator = false;
    for ch in raw.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
            previous_separator = false;
        } else if !previous_separator {
            normalized.push('_');
            previous_separator = true;
        }
    }
    let normalized = normalized.trim_matches('_').to_string();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn api_key_provider_id(account: &CodexAccount) -> String {
    account
        .api_provider_name
        .as_deref()
        .and_then(sanitize_provider_id)
        .unwrap_or_else(|| "api_key".to_string())
}

fn write_string_atomic(path: &Path, content: &str) -> Result<(), String> {
    write_bytes_atomic(path, content.as_bytes())
}

pub(super) fn write_bytes_atomic(path: &Path, content: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建目录失败: {}", error))?;
    }
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("codex-data");
    let tmp_path = path.with_file_name(format!(
        ".{}.codex-switcher-{:016x}.tmp",
        file_name,
        rand::random::<u64>()
    ));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut tmp_file = options
        .open(&tmp_path)
        .map_err(|error| format!("创建临时文件失败: {}", error))?;
    if let Err(error) = tmp_file
        .write_all(content)
        .and_then(|_| tmp_file.sync_all())
    {
        drop(tmp_file);
        let _ = fs::remove_file(&tmp_path);
        return Err(format!("写入临时文件失败: {}", error));
    }
    drop(tmp_file);

    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, ReplaceFileW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
            REPLACEFILE_WRITE_THROUGH,
        };

        let wide_path = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let wide_tmp_path = tmp_path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let replaced = unsafe {
            if path.is_file() {
                ReplaceFileW(
                    wide_path.as_ptr(),
                    wide_tmp_path.as_ptr(),
                    std::ptr::null(),
                    REPLACEFILE_WRITE_THROUGH,
                    std::ptr::null(),
                    std::ptr::null(),
                )
            } else {
                MoveFileExW(
                    wide_tmp_path.as_ptr(),
                    wide_path.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            }
        };
        if replaced == 0 {
            let error = std::io::Error::last_os_error();
            let _ = fs::remove_file(&tmp_path);
            return Err(format!("替换文件失败: {}", error));
        }
        Ok(())
    }

    #[cfg(not(windows))]
    {
        if let Err(error) = fs::rename(&tmp_path, path) {
            let _ = fs::remove_file(&tmp_path);
            return Err(format!("替换文件失败: {}", error));
        }
        Ok(())
    }
}

fn rollback_config_on_error<T>(
    result: Result<T, String>,
    config_path: &Path,
    config_before_update: &str,
) -> Result<T, String> {
    match result {
        Ok(value) => Ok(value),
        Err(database_error) => match write_string_atomic(config_path, config_before_update) {
            Ok(()) => Err(database_error),
            Err(rollback_error) => Err(format!(
                "{}；回滚 config.toml 失败: {}",
                database_error, rollback_error
            )),
        },
    }
}

#[cfg(test)]
mod tests;
