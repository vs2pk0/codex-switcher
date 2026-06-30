use super::{
    codex_restart_commands, codex_restart_delay_ms, AccountStore, ApiKeyAccountBindingInput,
    CodexAccount, CodexQuota,
};
use serde_json::json;
use std::fs;
use tempfile::tempdir;

fn test_store() -> (tempfile::TempDir, tempfile::TempDir, AccountStore) {
    let storage = tempdir().expect("storage tempdir");
    let codex = tempdir().expect("codex tempdir");
    let store = AccountStore::new(storage.path().to_path_buf(), codex.path().to_path_buf());
    (storage, codex, store)
}

#[test]
fn imports_token_json_and_persists_account() {
    let (_storage, _codex, store) = test_store();
    let input = json!({
        "email": "owner@example.com",
        "tokens": {
            "id_token": "id-token",
            "access_token": "access-token",
            "refresh_token": "refresh-token"
        }
    });

    let imported = store
        .import_from_json(&input.to_string())
        .expect("import succeeds");

    assert_eq!(imported.len(), 1);
    assert_eq!(imported[0].email, "owner@example.com");
    assert_eq!(store.list_accounts().expect("list accounts").len(), 1);
}

#[test]
fn imports_account_metadata_for_phone_expiry_and_binding() {
    let (_storage, _codex, store) = test_store();
    let input = json!({
        "email": "owner@example.com",
        "bound_phone": "+1 (724) 806-2018",
        "subscription_active_until": "2026-07-01T21:34:00Z",
        "access_token_expires_at": "2026-06-20T09:52:00Z",
        "tokens": {
            "id_token": "id-token",
            "access_token": "access-token",
            "refresh_token": "refresh-token"
        }
    });

    let imported = store
        .import_from_json(&input.to_string())
        .expect("import succeeds");

    assert_eq!(
        imported[0].bound_phone.as_deref(),
        Some("+1 (724) 806-2018")
    );
    assert_eq!(
        imported[0].subscription_active_until.as_deref(),
        Some("2026-07-01T21:34:00Z")
    );
    assert_eq!(
        imported[0].access_token_expires_at.as_deref(),
        Some("2026-06-20T09:52:00Z")
    );
}

#[test]
fn imports_access_token_exp_as_token_expiry() {
    let (_storage, _codex, store) = test_store();
    let access_token = "eyJhbGciOiJub25lIn0.eyJleHAiOjE3ODIzMDAwMDB9.signature";
    let input = json!({
        "email": "owner@example.com",
        "tokens": {
            "id_token": "id-token",
            "access_token": access_token,
            "refresh_token": "refresh-token"
        }
    });

    let imported = store
        .import_from_json(&input.to_string())
        .expect("import succeeds");

    assert_eq!(
        imported[0].access_token_expires_at.as_deref(),
        Some("1782300000")
    );
}

#[test]
fn imports_subscription_expiry_from_id_token_auth_payload() {
    let (_storage, _codex, store) = test_store();
    let id_token = concat!(
        "eyJhbGciOiJub25lIn0.",
        "eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsiY2hhdGdwdF9zdWJzY3JpcHRpb25fYWN0aXZlX3VudGlsIjoiMjAyNi0wNy0wMVQyMTozNDowMFoifX0.",
        "signature"
    );
    let input = json!({
        "email": "owner@example.com",
        "tokens": {
            "id_token": id_token,
            "access_token": "access-token",
            "refresh_token": "refresh-token"
        }
    });

    let imported = store
        .import_from_json(&input.to_string())
        .expect("import succeeds");

    assert_eq!(
        imported[0].subscription_active_until.as_deref(),
        Some("2026-07-01T21:34:00Z")
    );
}

#[test]
fn exports_sub2api_and_cpa_formats() {
    let (_storage, _codex, store) = test_store();
    let account = store
        .import_from_json(
            &json!({
                "email": "owner@example.com",
                "bound_phone": "+1 724",
                "subscription_active_until": "2026-07-01T21:34:00Z",
                "tokens": {
                    "id_token": "id-token",
                    "access_token": "access-token",
                    "refresh_token": "refresh-token"
                }
            })
            .to_string(),
        )
        .expect("import account")
        .remove(0);

    let sub2api = store
        .export_accounts(&[account.id.clone()], Some("sub2api"))
        .expect("export sub2api");
    let sub2api_value: serde_json::Value = serde_json::from_str(&sub2api).expect("sub2api json");
    assert_eq!(sub2api_value["type"], "sub2api-data");
    assert_eq!(sub2api_value["accounts"][0]["platform"], "openai");
    assert_eq!(
        sub2api_value["accounts"][0]["credentials"]["access_token"],
        "access-token"
    );
    assert_eq!(
        sub2api_value["accounts"][0]["credentials"]["subscription_expires_at"],
        "2026-07-01T21:34:00Z"
    );

    let cpa = store
        .export_accounts(&[account.id], Some("cpa"))
        .expect("export cpa");
    let cpa_value: serde_json::Value = serde_json::from_str(&cpa).expect("cpa json");
    assert_eq!(cpa_value["type"], "codex");
    assert_eq!(cpa_value["access_token"], "access-token");
    assert_eq!(cpa_value["bound_phone"], "+1 724");
}

#[test]
fn updates_account_from_editable_json() {
    let (_storage, _codex, store) = test_store();
    let account = store
        .import_from_json(
            &json!({
                "email": "owner@example.com",
                "tokens": {
                    "id_token": "id-token",
                    "access_token": "access-token",
                    "refresh_token": "refresh-token"
                }
            })
            .to_string(),
        )
        .expect("import account")
        .remove(0);

    let updated = store
        .update_account_from_json(
            &account.id,
            &json!({
                "id": account.id,
                "email": "owner@example.com",
                "account_name": "编辑后的账号",
                "tokens": {
                    "id_token": "id-token-2",
                    "access_token": "access-token-2",
                    "refresh_token": "refresh-token-2"
                },
                "created_at": account.created_at,
                "last_used": account.last_used
            })
            .to_string(),
        )
        .expect("update account json");

    assert_eq!(updated.account_name.as_deref(), Some("编辑后的账号"));
    assert_eq!(updated.tokens.access_token, "access-token-2");
}

#[test]
fn updates_account_from_exported_accounts_json() {
    let (_storage, _codex, store) = test_store();
    let account = store
        .import_from_json(
            &json!({
                "email": "owner@example.com",
                "tokens": {
                    "id_token": "id-token",
                    "access_token": "access-token",
                    "refresh_token": "refresh-token"
                }
            })
            .to_string(),
        )
        .expect("import account")
        .remove(0);

    let updated = store
        .update_account_from_json(
            &account.id,
            &json!({
                "app": "Codex Switcher",
                "format": "codex-switcher.accounts",
                "version": 1,
                "accounts": [
                    {
                        "id": account.id,
                        "email": "owner@example.com",
                        "account_name": "导出包里的账号",
                        "tokens": {
                            "id_token": "id-token-3",
                            "access_token": "access-token-3",
                            "refresh_token": "refresh-token-3"
                        },
                        "created_at": account.created_at,
                        "last_used": account.last_used
                    }
                ]
            })
            .to_string(),
        )
        .expect("update account from exported json");

    assert_eq!(updated.account_name.as_deref(), Some("导出包里的账号"));
    assert_eq!(updated.tokens.access_token, "access-token-3");
}

#[test]
fn updates_phone_and_exports_cockpit_tools_json() {
    let (_storage, _codex, store) = test_store();
    let account = store
        .import_from_json(
            &json!({
                "email": "owner@example.com",
                "subscription_active_until": "2026-07-01T21:34:00Z",
                "tokens": {
                    "id_token": "id-token",
                    "access_token": "access-token",
                    "refresh_token": "refresh-token"
                }
            })
            .to_string(),
        )
        .expect("import account")
        .remove(0);

    let updated = store
        .update_account_phone(&account.id, "+1 724 806 2018".to_string())
        .expect("update phone");
    let exported = store
        .export_accounts(&[updated.id.clone()], None)
        .expect("export account");
    let value: serde_json::Value = serde_json::from_str(&exported).expect("json export");

    assert_eq!(value["app"], "Codex Switcher");
    assert_eq!(value["format"], "codex-switcher.accounts");
    assert!(value["exported_at"].as_str().is_some());
    assert_eq!(value["accounts"][0]["bound_phone"], "+1 724 806 2018");
    assert_eq!(
        value["accounts"][0]["subscription_active_until"],
        "2026-07-01T21:34:00Z"
    );
    assert_eq!(
        value["accounts"][0]["tokens"]["access_token"],
        "access-token"
    );
}

#[test]
fn updates_account_profile_name_for_oauth_account() {
    let (_storage, _codex, store) = test_store();
    let account = store
        .import_from_json(
            &json!({
                "email": "owner@example.com",
                "tokens": {
                    "id_token": "id-token",
                    "access_token": "access-token",
                    "refresh_token": "refresh-token"
                }
            })
            .to_string(),
        )
        .expect("import account")
        .remove(0);

    let updated = store
        .update_account_profile(&account.id, Some("主账号".to_string()))
        .expect("update profile");

    assert_eq!(updated.account_name.as_deref(), Some("主账号"));
}

#[test]
fn saves_oauth_tokens_as_account() {
    let (_storage, _codex, store) = test_store();

    let account = store
        .save_oauth_tokens(
            "id-token".to_string(),
            "access-token".to_string(),
            Some("refresh-token".to_string()),
        )
        .expect("save oauth tokens");

    assert_eq!(account.tokens.access_token, "access-token");
    assert_eq!(
        account.tokens.refresh_token.as_deref(),
        Some("refresh-token")
    );
    assert_eq!(store.list_accounts().expect("list").len(), 1);
}

#[test]
fn save_oauth_tokens_refreshes_current_account_projection_by_email() {
    let (_storage, codex, store) = test_store();
    let old_account = store
        .import_from_json(
            &json!({
                "email": "owner@example.com",
                "tokens": {
                    "id_token": "old-id-token",
                    "access_token": "old-access-token",
                    "refresh_token": "old-refresh-token"
                }
            })
            .to_string(),
        )
        .expect("import account")
        .remove(0);
    store
        .switch_account(&old_account.id)
        .expect("switch old account");

    let updated = store
        .save_oauth_tokens(
            "eyJhbGciOiJub25lIn0.eyJlbWFpbCI6Im93bmVyQGV4YW1wbGUuY29tIn0.signature".to_string(),
            "new-access-token".to_string(),
            Some("new-refresh-token".to_string()),
        )
        .expect("refresh oauth tokens");

    assert_eq!(updated.id, old_account.id);
    assert_eq!(store.list_accounts().expect("list").len(), 1);
    let current = store.current_account().expect("current").expect("selected");
    assert_eq!(current.id, old_account.id);
    let auth_json = fs::read_to_string(codex.path().join("auth.json")).expect("auth json");
    let auth: serde_json::Value = serde_json::from_str(&auth_json).expect("valid auth json");
    assert_eq!(auth["tokens"]["access_token"], "new-access-token");
    assert_eq!(auth["tokens"]["refresh_token"], "new-refresh-token");
}

#[test]
fn rejects_url_as_api_key() {
    let (_storage, _codex, store) = test_store();

    let error = store
        .add_api_key_account(
            "https://relay.example/v1".to_string(),
            Some("https://relay.example/v1".to_string()),
            Some("Relay".to_string()),
            None,
            None,
        )
        .expect_err("URL api key should be rejected");

    assert!(error.contains("API Key 不能是 URL"));
}

#[test]
fn binds_api_key_to_oauth_account_and_writes_combined_projection() {
    let (_storage, codex, store) = test_store();
    let oauth = store
        .import_from_json(
            &json!({
                "email": "owner@example.com",
                "tokens": {
                    "id_token": "id-token",
                    "access_token": "access-token",
                    "refresh_token": "refresh-token"
                }
            })
            .to_string(),
        )
        .expect("import oauth")
        .remove(0);
    let api = store
        .add_api_key_account(
            "sk-bound-123456".to_string(),
            Some("https://relay.example/v1".to_string()),
            Some("Relay".to_string()),
            None,
            Some("Relay Key".to_string()),
        )
        .expect("add api key account");

    let updated = store
        .update_api_key_bound_oauth_account(&api.id, Some(oauth.id.clone()), false)
        .expect("bind oauth");
    let switched = store
        .switch_account(&updated.id)
        .expect("switch bound api key");

    assert_eq!(
        switched.bound_oauth_account_id.as_deref(),
        Some(oauth.id.as_str())
    );
    let auth_json = fs::read_to_string(codex.path().join("auth.json")).expect("auth json");
    let auth: serde_json::Value = serde_json::from_str(&auth_json).expect("valid auth json");
    assert!(auth["OPENAI_API_KEY"].is_null());
    assert!(auth.get("auth_mode").is_none());
    assert_eq!(auth["tokens"]["access_token"], "access-token");

    let config_toml = fs::read_to_string(codex.path().join("config.toml")).expect("config toml");
    assert!(config_toml.contains("model_provider = \"relay\""));
    assert!(config_toml.contains("experimental_bearer_token = \"sk-bound-123456\""));
    assert!(!config_toml.contains("env_key = \"OPENAI_API_KEY\""));
}

#[test]
fn adds_api_key_account_with_existing_oauth_binding() {
    let (_storage, _codex, store) = test_store();
    let oauth = store
        .import_from_json(
            &json!({
                "email": "owner@example.com",
                "tokens": {
                    "id_token": "id-token",
                    "access_token": "access-token",
                    "refresh_token": "refresh-token"
                }
            })
            .to_string(),
        )
        .expect("import oauth")
        .remove(0);

    let api = store
        .add_api_key_account_with_binding(ApiKeyAccountBindingInput {
            api_key: "sk-add-bound-123456".to_string(),
            api_base_url: Some("https://relay.example/v1".to_string()),
            api_provider_name: Some("Relay".to_string()),
            api_official_url: None,
            account_name: Some("Relay Key".to_string()),
            bound_oauth_account_id: Some(oauth.id.clone()),
            bound_oauth_use_local_gateway: false,
        })
        .expect("add bound api key account");

    assert_eq!(
        api.bound_oauth_account_id.as_deref(),
        Some(oauth.id.as_str())
    );
    assert!(!api.bound_oauth_use_local_gateway);
}

#[test]
fn unbinding_api_key_oauth_clears_cached_quota() {
    let (_storage, _codex, store) = test_store();
    let oauth = store
        .import_from_json(
            &json!({
                "email": "owner@example.com",
                "tokens": {
                    "id_token": "id-token",
                    "access_token": "access-token",
                    "refresh_token": "refresh-token"
                }
            })
            .to_string(),
        )
        .expect("import oauth")
        .remove(0);
    let api = store
        .add_api_key_account_with_binding(ApiKeyAccountBindingInput {
            api_key: "sk-quota-bound-123456".to_string(),
            api_base_url: Some("https://relay.example/v1".to_string()),
            api_provider_name: Some("Relay".to_string()),
            api_official_url: None,
            account_name: Some("Relay Key".to_string()),
            bound_oauth_account_id: Some(oauth.id.clone()),
            bound_oauth_use_local_gateway: false,
        })
        .expect("add bound api key account");
    store
        .update_account_quota(&api.id, test_quota())
        .expect("cache quota");

    let updated = store
        .update_api_key_bound_oauth_account(&api.id, None, false)
        .expect("unbind oauth");

    assert!(updated.bound_oauth_account_id.is_none());
    assert!(updated.quota.is_none());
    assert!(updated.quota_error.is_none());
    assert!(updated.usage_updated_at.is_none());
}

#[test]
fn deleting_bound_oauth_account_clears_api_key_binding_and_quota() {
    let (_storage, _codex, store) = test_store();
    let oauth = store
        .import_from_json(
            &json!({
                "email": "owner@example.com",
                "tokens": {
                    "id_token": "id-token",
                    "access_token": "access-token",
                    "refresh_token": "refresh-token"
                }
            })
            .to_string(),
        )
        .expect("import oauth")
        .remove(0);
    let api = store
        .add_api_key_account_with_binding(ApiKeyAccountBindingInput {
            api_key: "sk-delete-bound-123456".to_string(),
            api_base_url: Some("https://relay.example/v1".to_string()),
            api_provider_name: Some("Relay".to_string()),
            api_official_url: None,
            account_name: Some("Relay Key".to_string()),
            bound_oauth_account_id: Some(oauth.id.clone()),
            bound_oauth_use_local_gateway: false,
        })
        .expect("add bound api key account");
    store
        .update_account_quota(&api.id, test_quota())
        .expect("cache quota");

    store.delete_account(&oauth.id).expect("delete oauth");

    let updated = store
        .list_accounts()
        .expect("list accounts")
        .into_iter()
        .find(|account| account.id == api.id)
        .expect("api account remains");
    assert!(updated.bound_oauth_account_id.is_none());
    assert!(updated.quota.is_none());
    assert!(updated.quota_error.is_none());
    assert!(updated.usage_updated_at.is_none());
}

#[test]
fn imports_api_key_metadata_fields() {
    let (_storage, _codex, store) = test_store();
    let imported = store
        .import_from_json(
            &json!({
                "OPENAI_API_KEY": "sk-import-meta-123456",
                "api_base_url": "https://relay.example/v1",
                "api_provider_name": "Relay",
                "bound_phone": "+1 555 0000",
                "access_token_expires_at": "2026-06-20T09:52:00Z"
            })
            .to_string(),
        )
        .expect("import api key");

    assert_eq!(imported[0].bound_phone.as_deref(), Some("+1 555 0000"));
    assert_eq!(
        imported[0].access_token_expires_at.as_deref(),
        Some("2026-06-20T09:52:00Z")
    );
}

fn test_quota() -> CodexQuota {
    CodexQuota {
        hourly_percentage: 94,
        hourly_reset_time: Some(1_782_472_680),
        hourly_window_minutes: Some(300),
        hourly_window_present: Some(true),
        weekly_percentage: 97,
        weekly_reset_time: Some(1_783_003_740),
        weekly_window_minutes: Some(10_080),
        weekly_window_present: Some(true),
        reset_credits_available: None,
        raw_data: None,
    }
}

#[test]
fn switches_api_key_account_to_codex_auth_and_config() {
    let (_storage, codex, store) = test_store();
    let account = store
        .add_api_key_account(
            "sk-test-123456".to_string(),
            Some("https://relay.example/v1".to_string()),
            Some("Relay".to_string()),
            None,
            Some("Relay Key".to_string()),
        )
        .expect("add api key account");

    let switched = store.switch_account(&account.id).expect("switch account");

    assert_eq!(switched.id, account.id);
    let auth_json = fs::read_to_string(codex.path().join("auth.json")).expect("auth json");
    let auth: serde_json::Value = serde_json::from_str(&auth_json).expect("valid auth json");
    assert_eq!(auth["auth_mode"], "apikey");
    assert_eq!(auth["OPENAI_API_KEY"], "sk-test-123456");

    let config_toml = fs::read_to_string(codex.path().join("config.toml")).expect("config toml");
    assert!(config_toml.contains("model_provider = \"relay\""));
    assert!(config_toml.contains("base_url = \"https://relay.example/v1\""));
    assert!(config_toml.contains("experimental_bearer_token = \"sk-test-123456\""));
    assert!(!config_toml.contains("env_key = \"OPENAI_API_KEY\""));
    assert_eq!(
        store
            .current_account()
            .expect("current account")
            .map(|item: CodexAccount| item.id),
        Some(account.id)
    );
}

#[test]
fn delete_account_removes_account_and_clears_current() {
    let (_storage, _codex, store) = test_store();
    let account = store
        .add_api_key_account(
            "sk-delete-123456".to_string(),
            Some("https://relay.example/v1".to_string()),
            Some("Relay".to_string()),
            None,
            Some("Delete Key".to_string()),
        )
        .expect("add api key account");
    store.switch_account(&account.id).expect("switch account");

    store.delete_account(&account.id).expect("delete account");

    assert!(store.list_accounts().expect("list").is_empty());
    assert!(store.current_account().expect("current").is_none());
}

#[test]
fn restart_command_targets_codex_app_on_current_platform() {
    let (_stop_program, _stop_args, start_program, start_args) = codex_restart_commands();

    #[cfg(target_os = "macos")]
    {
        assert_eq!(start_program, "open");
        assert_eq!(start_args, vec!["-n", "-a", "Codex"]);
        assert!(codex_restart_delay_ms() >= 1000);
    }

    #[cfg(target_os = "windows")]
    {
        assert_eq!(start_program, "powershell");
        assert!(start_args.iter().any(|arg| arg.contains("Codex.exe")));
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        assert_eq!(start_program, "codex");
    }
}
