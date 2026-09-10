//! 本地服务（OpenCodex / API 服务）接入 Codex 实例后的 OAuth 账号绑定。
//!
//! 服务接入只负责把请求路由到本地代理，Codex 客户端仍然需要一份 ChatGPT 登录态（auth.json）才能正常使用。
//! 这里复用 API Key 账号「绑定 OAuth」的思路：
//! - API 服务：把 OAuth 账号绑定到代表本地 API 服务的 API Key 账号上（`bound_oauth_account_id`），
//!   切换到该账号时 auth.json 写入 OAuth Token、config.toml 写入 API 服务 Provider。
//! - OpenCodex：路由由 OpenCodex 自行注入 config.toml，这里只把 OAuth 登录态写入 auth.json。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::account::{is_usable_oauth_account, oauth_tokens_complete, AccountStore, CodexAccount};
use crate::{instances, switcher_account_dir, token_keeper};

/// 接入 Codex 实例的本地服务类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ServiceKind {
    #[serde(rename = "opencodex")]
    OpenCodex,
    ApiService,
}

impl ServiceKind {
    fn label(self) -> &'static str {
        match self {
            ServiceKind::OpenCodex => "OpenCodex",
            ServiceKind::ApiService => "API 服务",
        }
    }
}

/// 某个实例上服务当前绑定的 OAuth 账号。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceOauthBinding {
    pub kind: ServiceKind,
    pub instance_id: String,
    pub bound_account_id: Option<String>,
    pub bound_account_label: Option<String>,
}

fn account_label(account: &CodexAccount) -> String {
    account
        .account_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .or_else(|| Some(account.email.trim()).filter(|email| !email.is_empty()))
        .unwrap_or(account.id.as_str())
        .to_string()
}

fn instance_account_store(instance_id: &str) -> Result<AccountStore, String> {
    let instance = instances::resolve_instance(instance_id)?;
    Ok(AccountStore::new_for_instance(
        switcher_account_dir(),
        PathBuf::from(&instance.codex_home),
        &instance.id,
    ))
}

/// 按优先级列出可作为默认绑定的 OAuth 账号：
/// 1. 实例当前账号（OAuth 账号本身，或 API Key 账号已绑定的 OAuth 账号）；
/// 2. 账号列表中其余可用的 OAuth 账号（保持列表顺序）。
pub(crate) fn default_oauth_candidates(store: &AccountStore) -> Result<Vec<String>, String> {
    let accounts = store.list_accounts()?;
    let mut candidates: Vec<String> = Vec::new();
    if let Some(current) = store.current_account()? {
        let preferred = if current.auth_mode.as_deref() == Some("apikey") {
            current
                .bound_oauth_account_id
                .as_deref()
                .and_then(|id| accounts.iter().find(|account| account.id == id))
        } else {
            accounts.iter().find(|account| account.id == current.id)
        };
        // 实例当前账号即便被隐藏也优先（隐藏只影响列表展示），只要求 Token 齐备。
        if let Some(account) = preferred.filter(|account| oauth_tokens_complete(account)) {
            candidates.push(account.id.clone());
        }
    }
    for account in accounts
        .iter()
        .filter(|account| is_usable_oauth_account(account))
    {
        if !candidates.iter().any(|id| id == &account.id) {
            candidates.push(account.id.clone());
        }
    }
    Ok(candidates)
}

/// 挑选默认 OAuth 账号并完成 Token 续期，返回第一个可用的账号 ID；全部不可用时返回 `None`。
pub(crate) async fn prepare_default_oauth_account(
    instance_id: &str,
) -> Result<Option<String>, String> {
    let store = instance_account_store(instance_id)?;
    for candidate in default_oauth_candidates(&store)? {
        match token_keeper::ensure_fresh_access_token_for_store(
            &store,
            &candidate,
            "接入本地服务前 Token 需要续期",
        )
        .await
        {
            Ok(_) => return Ok(Some(candidate)),
            Err(error) => {
                eprintln!("[service-binding] 跳过无法续期的 OAuth 账号 {candidate}: {error}");
            }
        }
    }
    Ok(None)
}

/// 在普通线程里同步等待 [`prepare_default_oauth_account`]。
pub(crate) fn prepare_default_oauth_account_blocking(
    instance_id: &str,
) -> Result<Option<String>, String> {
    tauri::async_runtime::block_on(prepare_default_oauth_account(instance_id))
}

/// 读取实例上某个服务当前绑定的 OAuth 账号。
pub(crate) fn read_service_oauth_binding(
    kind: ServiceKind,
    instance_id: &str,
) -> Result<ServiceOauthBinding, String> {
    let store = instance_account_store(instance_id)?;
    let accounts = store.list_accounts()?;
    let bound_account_id = match kind {
        ServiceKind::ApiService => crate::api_service::find_api_service_account(&accounts)
            .and_then(|account| account.bound_oauth_account_id.clone()),
        ServiceKind::OpenCodex => {
            // OpenCodex 不经过 API Key 账号中介：绑定即实例当前记录的 OAuth 账号（同步 / 手动绑定时都会写入）。
            // 只有 auth.json 里的登录态确实属于该账号时才算已绑定，避免显示与实际不一致。
            store
                .current_account()?
                .filter(|account| account.auth_mode.as_deref() != Some("apikey"))
                .filter(|account| store.auth_json_matches_account(account))
                .map(|account| account.id)
        }
    };
    let bound_account_label = bound_account_id
        .as_deref()
        .and_then(|id| accounts.iter().find(|account| account.id == id))
        .map(account_label);
    Ok(ServiceOauthBinding {
        kind,
        instance_id: instance_id.to_string(),
        bound_account_id,
        bound_account_label,
    })
}

/// 把 OAuth 账号绑定到实例上的服务（`None` 表示取消绑定）。调用方需保证实例已停止或随后重启。
/// 返回最终绑定的 OAuth 账号（取消绑定时为 `None`）。
pub(crate) fn apply_service_oauth_binding(
    kind: ServiceKind,
    store: &AccountStore,
    oauth_account_id: Option<&str>,
    resync_current_account: bool,
) -> Result<Option<CodexAccount>, String> {
    let oauth_account_id = oauth_account_id.map(str::trim).filter(|id| !id.is_empty());
    match kind {
        ServiceKind::ApiService => {
            let api_account_id = crate::api_service::ensure_api_service_account_from_settings()?;
            // 本地网关模式：config.toml 的 Provider 保持 ChatGPT 登录态（requires_openai_auth = true，
            // Codex 界面显示 OAuth 账号），网关 API Key 通过 X-Api-Key 头传给 CLIProxyAPI。
            let updated = store.update_api_key_bound_oauth_account(
                &api_account_id,
                oauth_account_id.map(str::to_string),
                true,
            )?;
            // 实例当前正使用 API 服务账号（或 config.toml 已路由到 API 服务）时重新落盘 auth.json / config.toml，
            // 确保登录态与网关鉴权头立即生效。
            if resync_current_account
                && (store
                    .current_account()?
                    .is_some_and(|current| current.id == api_account_id)
                    || crate::api_service::codex_home_has_api_service_routing(store.codex_home()))
            {
                store.switch_account(&api_account_id)?;
            }
            let accounts = store.list_accounts()?;
            Ok(updated
                .bound_oauth_account_id
                .as_deref()
                .and_then(|id| accounts.iter().find(|account| account.id == id).cloned()))
        }
        ServiceKind::OpenCodex => store.apply_oauth_login_only(oauth_account_id),
    }
}

/// 已接入 OpenCodex 且绑定了 OAuth 登录态的实例：确保代理 Provider 处于 ChatGPT 登录模式。
/// 返回本次是否修改了 config.toml。
pub(crate) fn ensure_opencodex_login_mode(instance_id: &str) -> Result<bool, String> {
    let binding = read_service_oauth_binding(ServiceKind::OpenCodex, instance_id)?;
    if binding.bound_account_id.is_none() {
        return Ok(false);
    }
    let store = instance_account_store(instance_id)?;
    crate::account::set_active_provider_login_mode(store.codex_home(), true)
}

/// 同步配置时的自动绑定：挑选默认 OAuth 账号并绑定；返回给用户看的说明文字。
pub(crate) fn auto_bind_default_oauth(
    kind: ServiceKind,
    store: &AccountStore,
    oauth_account_id: Option<&str>,
) -> Result<String, String> {
    let Some(oauth_account_id) = oauth_account_id else {
        return Ok(format!(
            "未找到可用的 OAuth 账号，{}接入后请手动点击「绑定 OAuth」",
            kind.label()
        ));
    };
    let bound = apply_service_oauth_binding(kind, store, Some(oauth_account_id), false)?
        .ok_or_else(|| "绑定 OAuth 账号失败：账号不存在".to_string())?;
    Ok(format!("已绑定 OAuth 账号「{}」", account_label(&bound)))
}

#[tauri::command]
pub fn codex_get_service_oauth_binding(
    kind: ServiceKind,
    instance_id: Option<String>,
) -> Result<ServiceOauthBinding, String> {
    let instance_id = instance_id.unwrap_or_else(|| instances::DEFAULT_INSTANCE_ID.to_string());
    read_service_oauth_binding(kind, &instance_id)
}

/// 更换 / 取消服务绑定的 OAuth 账号：先续期 Token，再在实例停止状态下写入，最后按原状态决定是否重启。
#[tauri::command]
pub async fn codex_update_service_oauth_binding(
    kind: ServiceKind,
    instance_id: Option<String>,
    oauth_account_id: Option<String>,
) -> Result<ServiceOauthBinding, String> {
    let instance_id = instance_id.unwrap_or_else(|| instances::DEFAULT_INSTANCE_ID.to_string());
    let oauth_account_id = oauth_account_id
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty());
    if let Some(oauth_account_id) = oauth_account_id.as_deref() {
        let store = instance_account_store(&instance_id)?;
        token_keeper::ensure_fresh_access_token_for_store(
            &store,
            oauth_account_id,
            "绑定 OAuth 账号前 Token 需要续期",
        )
        .await?;
    }
    let task_instance_id = instance_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        instances::run_with_instance_restarted_if_running(&task_instance_id, |instance| {
            let store = AccountStore::new_for_instance(
                switcher_account_dir(),
                PathBuf::from(&instance.codex_home),
                &instance.id,
            );
            apply_service_oauth_binding(kind, &store, oauth_account_id.as_deref(), true)?;
            Ok(())
        })?;
        read_service_oauth_binding(kind, &task_instance_id)
    })
    .await
    .map_err(|error| format!("更新服务 OAuth 绑定任务失败: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    fn store() -> (tempfile::TempDir, tempfile::TempDir, AccountStore) {
        let storage = tempdir().expect("storage");
        let codex = tempdir().expect("codex");
        let store = AccountStore::new(storage.path().to_path_buf(), codex.path().to_path_buf());
        (storage, codex, store)
    }

    fn import_oauth(store: &AccountStore, email: &str, refresh: &str) -> CodexAccount {
        store
            .import_from_json(
                &json!({
                    "email": email,
                    "tokens": {"id_token": "id", "access_token": format!("access-{email}"), "refresh_token": refresh}
                })
                .to_string(),
            )
            .unwrap()
            .remove(0)
    }

    #[test]
    fn default_candidates_prefer_current_account_then_first_usable() {
        let (_storage, _codex, store) = store();
        let first = import_oauth(&store, "first@example.com", "refresh-1");
        let second = import_oauth(&store, "second@example.com", "refresh-2");
        let broken = import_oauth(&store, "broken@example.com", "");

        // 没有当前账号：按账号列表顺序（最新导入在前），跳过缺少 refresh_token 的账号
        let candidates = default_oauth_candidates(&store).unwrap();
        assert_eq!(candidates, vec![second.id.clone(), first.id.clone()]);
        assert!(!candidates.contains(&broken.id));

        // 当前账号优先
        store.switch_account(&first.id).unwrap();
        let candidates = default_oauth_candidates(&store).unwrap();
        assert_eq!(candidates, vec![first.id.clone(), second.id.clone()]);

        // 当前账号是绑定了 OAuth 的 API Key 账号时，优先其绑定的 OAuth 账号
        let api = store
            .add_api_key_account("sk-test".to_string(), None, None, None, None)
            .unwrap();
        store
            .update_api_key_bound_oauth_account(&api.id, Some(first.id.clone()), false)
            .unwrap();
        store.switch_account(&api.id).unwrap();
        let candidates = default_oauth_candidates(&store).unwrap();
        assert_eq!(candidates, vec![first.id.clone(), second.id.clone()]);
        // OAuth 候选中不包含 API Key 账号自身
        assert!(!candidates.contains(&api.id));
    }

    #[test]
    fn service_kind_serializes_as_kebab_case() {
        assert_eq!(
            serde_json::to_string(&ServiceKind::ApiService).unwrap(),
            "\"api-service\""
        );
        assert_eq!(
            serde_json::from_str::<ServiceKind>("\"opencodex\"").unwrap(),
            ServiceKind::OpenCodex
        );
    }
}
