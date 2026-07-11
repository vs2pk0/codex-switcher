use super::{
    codex_restart_commands, codex_restart_delay_ms, rollback_config_on_error, AccountStore,
    ApiKeyAccountBindingInput, CodexAccount, CodexQuota,
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
fn database_failure_rolls_back_the_model_config() {
    let codex = tempdir().expect("codex tempdir");
    let config_path = codex.path().join("config.toml");
    fs::write(&config_path, "model = \"changed\"\n").expect("write changed config");

    let error = rollback_config_on_error::<()>(
        Err("保存账号库失败".to_string()),
        &config_path,
        "model = \"original\"\n",
    )
    .expect_err("surface database failure");

    assert_eq!(error, "保存账号库失败");
    assert_eq!(
        fs::read_to_string(&config_path).expect("read rolled back config"),
        "model = \"original\"\n"
    );
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
fn imports_flat_token_account_from_root_array() {
    let (_storage, _codex, store) = test_store();
    let input = json!([
        {
            "id_token": "id-token",
            "access_token": "access-token",
            "refresh_token": "refresh-token",
            "account_id": "remote-account-id",
            "email": "owner@example.com",
            "type": "codex",
            "expired": "2026-07-11T11:13:41.000Z"
        }
    ]);

    let imported = store
        .import_from_json(&input.to_string())
        .expect("import root array account");

    assert_eq!(imported.len(), 1);
    assert_eq!(imported[0].email, "owner@example.com");
    assert_eq!(imported[0].tokens.access_token, "access-token");
    assert_eq!(
        imported[0].access_token_expires_at.as_deref(),
        Some("2026-07-11T11:13:41.000Z")
    );
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
fn update_account_json_rejects_an_existing_account_id() {
    let (_storage, _codex, store) = test_store();
    let first = store
        .import_from_json(
            &json!({
                "email": "first@example.com",
                "tokens": {
                    "id_token": "first-id-token",
                    "access_token": "first-access-token",
                    "refresh_token": "first-refresh-token"
                }
            })
            .to_string(),
        )
        .expect("import first account")
        .remove(0);
    let second = store
        .import_from_json(
            &json!({
                "email": "second@example.com",
                "tokens": {
                    "id_token": "second-id-token",
                    "access_token": "second-access-token",
                    "refresh_token": "second-refresh-token"
                }
            })
            .to_string(),
        )
        .expect("import second account")
        .remove(0);
    let mut editable = serde_json::to_value(&first).expect("serialize first account");
    editable["id"] = json!(second.id.clone());

    let error = store
        .update_account_from_json(&first.id, &editable.to_string())
        .expect_err("duplicate account id must be rejected");
    assert!(error.contains("账号 ID 已存在"));

    let accounts = store.list_accounts().expect("list accounts");
    assert_eq!(accounts.len(), 2);
    assert!(accounts.iter().any(|account| {
        account.id == first.id && account.tokens.access_token == "first-access-token"
    }));
    assert!(accounts.iter().any(|account| {
        account.id == second.id && account.tokens.access_token == "second-access-token"
    }));
}

#[test]
fn fallback_api_key_json_validation_does_not_mutate_accounts_on_error() {
    let (storage, _codex, store) = test_store();
    let oauth = store
        .import_from_json(
            &json!({
                "email": "fallback-owner@example.com",
                "tokens": {
                    "id_token": "fallback-id-token",
                    "access_token": "fallback-access-token",
                    "refresh_token": "fallback-refresh-token"
                }
            })
            .to_string(),
        )
        .expect("import fallback owner")
        .remove(0);
    let existing_api = store
        .add_api_key_account(
            "sk-existing-fallback-123456".to_string(),
            Some("https://existing.example/v1".to_string()),
            Some("Existing Fallback".to_string()),
            None,
            None,
        )
        .expect("add existing fallback api");
    let database_path = storage.path().join("accounts.json");
    let original_database = fs::read(&database_path).expect("read original database");

    let duplicate_error = store
        .update_account_from_json(
            &oauth.id,
            &json!({
                "id": existing_api.id,
                "OPENAI_API_KEY": "sk-existing-fallback-123456",
                "default_model": "gpt-5.5"
            })
            .to_string(),
        )
        .expect_err("fallback duplicate id must fail");
    assert!(duplicate_error.contains("账号 ID 已存在"));
    assert_eq!(
        fs::read(&database_path).expect("read database after duplicate error"),
        original_database
    );

    let invalid_model_error = store
        .update_account_from_json(
            &oauth.id,
            &json!({
                "id": "fallback-invalid-model",
                "OPENAI_API_KEY": "sk-invalid-model-123456",
                "default_model": "invalid\nmodel"
            })
            .to_string(),
        )
        .expect_err("fallback invalid model must fail");
    assert!(invalid_model_error.contains("控制字符"));
    assert_eq!(
        fs::read(&database_path).expect("read database after invalid model error"),
        original_database
    );
}

#[test]
fn renaming_oauth_account_updates_bound_api_key_references() {
    let (_storage, codex, store) = test_store();
    let oauth = store
        .import_from_json(
            &json!({
                "email": "bound-owner@example.com",
                "tokens": {
                    "id_token": "bound-id-token",
                    "access_token": "bound-access-token",
                    "refresh_token": "bound-refresh-token"
                }
            })
            .to_string(),
        )
        .expect("import bound oauth")
        .remove(0);
    let api = store
        .add_api_key_account_with_binding(ApiKeyAccountBindingInput {
            api_key: "sk-bound-rename-123456".to_string(),
            api_base_url: Some("https://relay.example/v1".to_string()),
            api_provider_name: Some("Bound Relay".to_string()),
            api_official_url: None,
            account_name: None,
            bound_oauth_account_id: Some(oauth.id.clone()),
            bound_oauth_use_local_gateway: false,
        })
        .expect("add bound api account");
    store.switch_account(&api.id).expect("switch bound api");

    let mut editable = serde_json::to_value(&oauth).expect("serialize oauth account");
    editable["id"] = json!("renamed-bound-oauth");
    editable["tokens"]["access_token"] = json!("renamed-access-token");
    let renamed = store
        .update_account_from_json(&oauth.id, &editable.to_string())
        .expect("rename bound oauth");
    assert_eq!(renamed.id, "renamed-bound-oauth");

    let accounts = store.list_accounts().expect("list accounts");
    let bound_api = accounts
        .iter()
        .find(|account| account.id == api.id)
        .expect("bound api remains");
    assert_eq!(
        bound_api.bound_oauth_account_id.as_deref(),
        Some("renamed-bound-oauth")
    );
    let auth_json = fs::read_to_string(codex.path().join("auth.json")).expect("read auth json");
    let auth: serde_json::Value = serde_json::from_str(&auth_json).expect("parse auth json");
    assert_eq!(auth["tokens"]["access_token"], "renamed-access-token");
}

#[test]
fn updates_account_from_root_array_flat_token_json() {
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
            &json!([
                {
                    "id_token": "id-token-array",
                    "access_token": "access-token-array",
                    "refresh_token": "refresh-token-array",
                    "account_id": "remote-account-id",
                    "email": "owner@example.com",
                    "type": "codex",
                    "expired": "2026-07-11T11:13:41.000Z"
                }
            ])
            .to_string(),
        )
        .expect("update account from root array");

    assert_eq!(updated.email, "owner@example.com");
    assert_eq!(updated.id, account.id);
    assert_eq!(updated.tokens.access_token, "access-token-array");
    assert_eq!(
        updated.access_token_expires_at.as_deref(),
        Some("2026-07-11T11:13:41.000Z")
    );
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
fn detects_bound_api_key_current_account_from_codex_config() {
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
    let api = store
        .update_api_key_bound_oauth_account(&api.id, Some(oauth.id.clone()), false)
        .expect("bind oauth");
    store.switch_account(&oauth.id).expect("switch oauth first");

    fs::write(
        codex.path().join("auth.json"),
        json!({
            "OPENAI_API_KEY": null,
            "email": "owner@example.com",
            "tokens": {
                "id_token": "id-token",
                "access_token": "access-token",
                "refresh_token": "refresh-token"
            }
        })
        .to_string(),
    )
    .expect("write auth json");
    fs::write(
        codex.path().join("config.toml"),
        r#"
model_provider = "relay"

[model_providers.relay]
base_url = "https://relay.example/v1"
experimental_bearer_token = "sk-bound-123456"
"#,
    )
    .expect("write config");

    let detected = store
        .detect_current_account_from_codex_config()
        .expect("detect current")
        .expect("matched account");

    assert_eq!(detected.id, api.id);
    assert_eq!(
        store
            .current_account()
            .expect("current")
            .map(|account| account.id),
        Some(api.id)
    );
}

#[test]
fn detects_oauth_current_account_from_auth_json() {
    let (_storage, codex, store) = test_store();
    store
        .import_from_json(
            &json!({
                "email": "first@example.com",
                "tokens": {
                    "id_token": "first-id-token",
                    "access_token": "first-access-token",
                    "refresh_token": "first-refresh-token"
                }
            })
            .to_string(),
        )
        .expect("import first");
    let second = store
        .import_from_json(
            &json!({
                "email": "second@example.com",
                "tokens": {
                    "id_token": "second-id-token",
                    "access_token": "second-access-token",
                    "refresh_token": "second-refresh-token"
                }
            })
            .to_string(),
        )
        .expect("import second")
        .remove(0);
    fs::write(
        codex.path().join("auth.json"),
        json!({
            "OPENAI_API_KEY": null,
            "email": "second@example.com",
            "tokens": {
                "id_token": "second-id-token",
                "access_token": "second-access-token",
                "refresh_token": "second-refresh-token"
            }
        })
        .to_string(),
    )
    .expect("write auth json");
    fs::write(
        codex.path().join("config.toml"),
        "model_provider = \"openai\"\n",
    )
    .expect("write config");

    let detected = store
        .detect_current_account_from_codex_config()
        .expect("detect current")
        .expect("matched account");

    assert_eq!(detected.id, second.id);
}

#[test]
fn detects_oauth_current_account_when_official_provider_has_base_url() {
    let (_storage, codex, store) = test_store();
    store
        .add_api_key_account(
            "sk-official-like-123456".to_string(),
            Some("https://api.openai.com/v1".to_string()),
            Some("Official Relay".to_string()),
            None,
            Some("Official-like API Key".to_string()),
        )
        .expect("add api key account");
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
    fs::write(
        codex.path().join("auth.json"),
        json!({
            "OPENAI_API_KEY": null,
            "email": "owner@example.com",
            "tokens": {
                "id_token": "id-token",
                "access_token": "access-token",
                "refresh_token": "refresh-token"
            }
        })
        .to_string(),
    )
    .expect("write auth json");
    fs::write(
        codex.path().join("config.toml"),
        r#"
model_provider = "openai"

[model_providers.openai]
base_url = "https://api.openai.com/v1"
"#,
    )
    .expect("write config");

    let detected = store
        .detect_current_account_from_codex_config()
        .expect("detect current")
        .expect("matched account");

    assert_eq!(detected.id, oauth.id);
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
                "default_model": "gpt-5.6-sol",
                "bound_phone": "+1 555 0000",
                "access_token_expires_at": "2026-06-20T09:52:00Z"
            })
            .to_string(),
        )
        .expect("import api key");

    assert_eq!(imported[0].bound_phone.as_deref(), Some("+1 555 0000"));
    assert_eq!(imported[0].default_model.as_deref(), Some("gpt-5.6-sol"));
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
        reset_credits: Vec::new(),
        reset_credits_next_expires_at: None,
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
fn api_key_model_access_check_is_strict_and_read_only() {
    let (storage, codex, store) = test_store();
    let current = store
        .add_api_key_account(
            "sk-access-current-123456".to_string(),
            Some("https://current.example/v1".to_string()),
            Some("Access Current".to_string()),
            None,
            None,
        )
        .expect("add current access account");
    let other = store
        .add_api_key_account(
            "sk-access-other-123456".to_string(),
            Some("https://other.example/v1".to_string()),
            Some("Access Other".to_string()),
            None,
            None,
        )
        .expect("add other access account");
    store
        .switch_account(&current.id)
        .expect("switch current access account");

    let accounts_path = storage.path().join("accounts.json");
    let auth_path = codex.path().join("auth.json");
    let config_path = codex.path().join("config.toml");
    let accounts_before = fs::read(&accounts_path).expect("read accounts before access checks");
    let auth_before = fs::read(&auth_path).expect("read auth before access checks");
    let config_before = fs::read(&config_path).expect("read config before access checks");
    assert!(store
        .check_api_key_model_access(&current.id)
        .expect("check current access"));
    assert!(!store
        .check_api_key_model_access(&other.id)
        .expect("check other access"));
    assert_eq!(
        fs::read(&accounts_path).expect("read accounts after access checks"),
        accounts_before
    );
    assert_eq!(
        fs::read(&auth_path).expect("read auth after access checks"),
        auth_before
    );
    assert_eq!(
        fs::read(&config_path).expect("read config after access checks"),
        config_before
    );

    let mut mismatched_base = fs::read_to_string(&config_path)
        .expect("read config for base mismatch")
        .parse::<toml_edit::Document>()
        .expect("parse config for base mismatch");
    mismatched_base["model_providers"]["access_current"]["base_url"] =
        toml_edit::value("https://mismatch.example/v1");
    fs::write(&config_path, mismatched_base.to_string()).expect("write mismatched base");
    let mismatched_base_before = fs::read(&config_path).expect("read mismatched base config");
    assert!(!store
        .check_api_key_model_access(&current.id)
        .expect("check mismatched base access"));
    assert_eq!(
        fs::read(&config_path).expect("read config after base check"),
        mismatched_base_before
    );

    fs::write(&config_path, &config_before).expect("restore current config");
    fs::write(
        &auth_path,
        json!({
            "auth_mode": "apikey",
            "OPENAI_API_KEY": "sk-access-other-123456"
        })
        .to_string(),
    )
    .expect("write conflicting auth key");
    let conflicting_auth_before = fs::read(&auth_path).expect("read conflicting auth");
    assert!(!store
        .check_api_key_model_access(&current.id)
        .expect("check provider auth conflict"));
    assert_eq!(
        fs::read(&auth_path).expect("read auth after conflict check"),
        conflicting_auth_before
    );
    assert_eq!(
        fs::read(&config_path).expect("read config after conflict check"),
        config_before
    );
}

#[test]
fn stale_cached_current_account_cannot_overwrite_an_external_api_key_projection() {
    let (storage, codex, store) = test_store();
    let stale = store
        .add_api_key_account(
            "sk-stale-a-123456".to_string(),
            Some("https://a.example/v1".to_string()),
            Some("Stale A".to_string()),
            None,
            None,
        )
        .expect("add stale account");
    let actual = store
        .add_api_key_account(
            "sk-stale-b-123456".to_string(),
            Some("https://b.example/v1".to_string()),
            Some("Stale B".to_string()),
            None,
            None,
        )
        .expect("add actual account");
    store
        .switch_account(&stale.id)
        .expect("cache stale account as current");

    fs::write(
        codex.path().join("auth.json"),
        json!({
            "auth_mode": "apikey",
            "OPENAI_API_KEY": "sk-stale-b-123456"
        })
        .to_string(),
    )
    .expect("write external auth projection");
    fs::write(
        codex.path().join("config.toml"),
        r#"model = "external-model"
model_provider = "stale_b"

[model_providers.stale_b]
base_url = "https://b.example/v1"
experimental_bearer_token = "sk-stale-b-123456"
"#,
    )
    .expect("write external config projection");

    assert!(!store
        .check_api_key_model_access(&stale.id)
        .expect("check stale account access"));
    assert!(store
        .check_api_key_model_access(&actual.id)
        .expect("check actual account access"));
    let accounts_path = storage.path().join("accounts.json");
    let auth_path = codex.path().join("auth.json");
    let config_path = codex.path().join("config.toml");
    let accounts_before = fs::read(&accounts_path).expect("read accounts before rejected save");
    let auth_before = fs::read(&auth_path).expect("read auth before rejected save");
    let config_before = fs::read(&config_path).expect("read config before rejected save");

    let error = store
        .update_api_key_default_model(&stale.id, "gpt-5.6-sol".to_string())
        .expect_err("reject stale account save");
    assert!(error.contains("当前 Codex 配置不是该 API Key 账号"));
    assert_eq!(
        fs::read(&accounts_path).expect("read accounts after rejected save"),
        accounts_before
    );
    assert_eq!(
        fs::read(&auth_path).expect("read auth after rejected save"),
        auth_before
    );
    assert_eq!(
        fs::read(&config_path).expect("read config after rejected save"),
        config_before
    );
    assert_eq!(
        store
            .current_account()
            .expect("read cached current account")
            .map(|account| account.id),
        Some(stale.id)
    );
}

#[test]
fn actual_api_key_projection_resynchronizes_cached_current_and_replaces_stale_backup() {
    let (_storage, codex, store) = test_store();
    let stale = store
        .add_api_key_account(
            "sk-sync-b-123456".to_string(),
            Some("https://b.example/v1".to_string()),
            Some("Sync B".to_string()),
            None,
            None,
        )
        .expect("add stale sync account");
    let actual = store
        .add_api_key_account(
            "sk-sync-a-123456".to_string(),
            Some("https://a.example/v1".to_string()),
            Some("Sync A".to_string()),
            None,
            None,
        )
        .expect("add actual sync account");
    store
        .switch_account(&stale.id)
        .expect("switch stale sync account");
    fs::write(
        codex.path().join("config.toml"),
        r#"model = "stale-baseline"
model_provider = "sync_b"
network_access = "stale-baseline"

[model_providers.sync_b]
base_url = "https://b.example/v1"
experimental_bearer_token = "sk-sync-b-123456"
"#,
    )
    .expect("write stale managed baseline");
    store
        .update_api_key_default_model(&stale.id, "gpt-5.6-sol".to_string())
        .expect("create stale managed backup");

    fs::write(
        codex.path().join("auth.json"),
        json!({
            "auth_mode": "apikey",
            "OPENAI_API_KEY": "sk-sync-a-123456"
        })
        .to_string(),
    )
    .expect("write actual auth projection");
    fs::write(
        codex.path().join("config.toml"),
        r#"model = "actual-baseline"
model_provider = "sync_a"
network_access = "actual-baseline"

[model_providers.sync_a]
base_url = "https://a.example/v1"
experimental_bearer_token = "sk-sync-a-123456"
"#,
    )
    .expect("write actual config projection");

    let updated = store
        .update_api_key_default_model(&actual.id, "gpt-5.5".to_string())
        .expect("save model for actual projection");
    assert_eq!(updated.default_model.as_deref(), Some("gpt-5.5"));
    assert_eq!(
        store
            .current_account()
            .expect("read resynchronized current account")
            .map(|account| account.id),
        Some(actual.id)
    );

    let oauth = store
        .import_from_json(
            &json!({
                "email": "sync-restore@example.com",
                "tokens": {
                    "id_token": "id-token",
                    "access_token": "access-token",
                    "refresh_token": "refresh-token"
                }
            })
            .to_string(),
        )
        .expect("import sync restore oauth")
        .remove(0);
    store.switch_account(&oauth.id).expect("switch sync oauth");
    let restored = fs::read_to_string(codex.path().join("config.toml"))
        .expect("read synchronized restoration")
        .parse::<toml_edit::Document>()
        .expect("parse synchronized restoration");
    assert_eq!(restored["model"].as_str(), Some("actual-baseline"));
    assert_eq!(restored["network_access"].as_str(), Some("actual-baseline"));
}

#[test]
fn bound_api_keys_with_same_oauth_and_base_url_still_require_exact_bearer_token() {
    let (storage, codex, store) = test_store();
    let oauth = store
        .import_from_json(
            &json!({
                "email": "shared-bound@example.com",
                "tokens": {
                    "id_token": "id-token",
                    "access_token": "shared-access-token",
                    "refresh_token": "shared-refresh-token"
                }
            })
            .to_string(),
        )
        .expect("import shared bound oauth")
        .remove(0);
    let first = store
        .add_api_key_account(
            "sk-bound-first-123456".to_string(),
            Some("https://shared.example/v1".to_string()),
            Some("Bound First".to_string()),
            None,
            None,
        )
        .expect("add first bound api key");
    let first = store
        .update_api_key_bound_oauth_account(&first.id, Some(oauth.id.clone()), false)
        .expect("bind first api key");
    let second = store
        .add_api_key_account(
            "sk-bound-second-123456".to_string(),
            Some("https://shared.example/v1".to_string()),
            Some("Bound Second".to_string()),
            None,
            None,
        )
        .expect("add second bound api key");
    let second = store
        .update_api_key_bound_oauth_account(&second.id, Some(oauth.id.clone()), false)
        .expect("bind second api key");
    store
        .switch_account(&first.id)
        .expect("switch first bound api key");
    assert!(store
        .check_api_key_model_access(&first.id)
        .expect("check first bound access"));
    assert!(!store
        .check_api_key_model_access(&second.id)
        .expect("check second bound access"));

    let config_path = codex.path().join("config.toml");
    let mut config = fs::read_to_string(&config_path)
        .expect("read first bound config")
        .parse::<toml_edit::Document>()
        .expect("parse first bound config");
    config["model_providers"]["bound_first"]["experimental_bearer_token"] =
        toml_edit::value("sk-bound-second-123456");
    fs::write(&config_path, config.to_string()).expect("write second bearer into first provider");
    assert!(!store
        .check_api_key_model_access(&first.id)
        .expect("reject first bound key mismatch"));
    assert!(store
        .check_api_key_model_access(&second.id)
        .expect("recognize exact second bound key"));

    let accounts_path = storage.path().join("accounts.json");
    let auth_path = codex.path().join("auth.json");
    let accounts_before = fs::read(&accounts_path).expect("read bound accounts before rejection");
    let auth_before = fs::read(&auth_path).expect("read bound auth before rejection");
    let config_before = fs::read(&config_path).expect("read bound config before rejection");
    let error = store
        .update_api_key_default_model(&first.id, "gpt-5.6-sol".to_string())
        .expect_err("reject mismatched bound bearer save");
    assert!(error.contains("当前 Codex 配置不是该 API Key 账号"));
    assert_eq!(
        fs::read(&accounts_path).expect("read bound accounts after rejection"),
        accounts_before
    );
    assert_eq!(
        fs::read(&auth_path).expect("read bound auth after rejection"),
        auth_before
    );
    assert_eq!(
        fs::read(&config_path).expect("read bound config after rejection"),
        config_before
    );
}

#[test]
fn default_model_requires_current_account_and_applies_gpt_5_6_config() {
    let (_storage, codex, store) = test_store();
    let current = store
        .add_api_key_account(
            "sk-current-123456".to_string(),
            Some("https://current.example/v1".to_string()),
            Some("Current".to_string()),
            None,
            None,
        )
        .expect("add current account");
    store
        .switch_account(&current.id)
        .expect("switch current account");

    let target = store
        .add_api_key_account(
            "sk-gpt56-123456".to_string(),
            Some("https://relay.example/sub2api".to_string()),
            Some("Custom".to_string()),
            None,
            None,
        )
        .expect("add target account");
    let config_before_rejected_update =
        fs::read(codex.path().join("config.toml")).expect("read config before rejected update");
    let error = store
        .update_api_key_default_model(&target.id, "gpt-5.6-sol".to_string())
        .expect_err("reject non-current default model");
    assert!(error.contains("当前 Codex 配置不是该 API Key 账号"));
    assert_eq!(
        fs::read(codex.path().join("config.toml")).expect("read config after rejected update"),
        config_before_rejected_update
    );
    assert!(!store
        .check_api_key_model_access(&target.id)
        .expect("check non-current access"));

    store
        .switch_account(&target.id)
        .expect("switch target account");
    assert!(store
        .check_api_key_model_access(&target.id)
        .expect("check current access"));
    let updated = store
        .update_api_key_default_model(&target.id, "gpt-5.6-sol".to_string())
        .expect("save current default model");
    assert_eq!(updated.default_model.as_deref(), Some("gpt-5.6-sol"));
    let config = fs::read_to_string(codex.path().join("config.toml")).expect("read gpt 5.6 config");
    let document = config
        .parse::<toml_edit::Document>()
        .expect("parse gpt 5.6 config");
    assert_eq!(document["model"].as_str(), Some("gpt-5.6-sol"));
    assert_eq!(document["model_reasoning_effort"].as_str(), Some("high"));
    assert_eq!(document["disable_response_storage"].as_bool(), Some(true));
    assert_eq!(document["network_access"].as_str(), Some("enabled"));
    assert_eq!(
        document["windows_wsl_setup_acknowledged"].as_bool(),
        Some(true)
    );
    assert_eq!(document["requires_openai_auth"].as_bool(), Some(true));
    assert_eq!(document["features"]["goals"].as_bool(), Some(true));
    assert_eq!(document["features"]["js_repl"].as_bool(), Some(false));
    assert_eq!(document["features"]["memories"].as_bool(), Some(true));
    assert!(document["features"].get("websocket_v2").is_none());
    let provider = &document["model_providers"]["custom"];
    assert_eq!(
        provider["base_url"].as_str(),
        Some("https://relay.example/sub2api")
    );
    assert!(provider.get("supports_websockets").is_none());
    assert!(provider.get("websocket_v2").is_none());

    let auth_path = codex.path().join("auth.json");
    let mut auth_value: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&auth_path).expect("read current auth before model update"),
    )
    .expect("parse current auth");
    auth_value["sentinel"] = json!("must-stay-byte-for-byte");
    let auth_before_model_update = serde_json::to_vec_pretty(&auth_value).expect("serialize auth");
    fs::write(&auth_path, &auth_before_model_update).expect("write auth sentinel");

    store
        .update_api_key_default_model(&target.id, "gpt-5.6-sol-preview".to_string())
        .expect("update current default model");
    assert_eq!(
        fs::read(&auth_path).expect("read auth after model update"),
        auth_before_model_update,
        "setting a default model must not rewrite auth.json"
    );
    let current_config = fs::read_to_string(codex.path().join("config.toml"))
        .expect("read immediately updated current config");
    assert!(current_config.contains("model = \"gpt-5.6-sol-preview\""));

    let persisted = store
        .list_accounts()
        .expect("list persisted accounts")
        .into_iter()
        .find(|account| account.id == target.id)
        .expect("target persisted");
    assert_eq!(
        persisted.default_model.as_deref(),
        Some("gpt-5.6-sol-preview")
    );

    let exported = store
        .export_accounts(std::slice::from_ref(&target.id), None)
        .expect("export target account");
    let exported: serde_json::Value = serde_json::from_str(&exported).expect("parse export");
    assert_eq!(
        exported["accounts"][0]["default_model"],
        "gpt-5.6-sol-preview"
    );

    store
        .update_api_key_default_model(&target.id, "gpt-5.5".to_string())
        .expect("switch current account to ordinary model");
    let ordinary_config =
        fs::read_to_string(codex.path().join("config.toml")).expect("read ordinary model config");
    let ordinary_document = ordinary_config
        .parse::<toml_edit::Document>()
        .expect("parse ordinary model config");
    assert_eq!(ordinary_document["model"].as_str(), Some("gpt-5.5"));
    assert!(ordinary_document.get("model_reasoning_effort").is_none());
    assert!(ordinary_document.get("disable_response_storage").is_none());
    assert!(ordinary_document.get("network_access").is_none());
    for key in ["goals", "js_repl", "memories"] {
        assert!(ordinary_document
            .get("features")
            .and_then(toml_edit::Item::as_table_like)
            .and_then(|features| features.get(key))
            .is_none());
    }

    let plain = store
        .add_api_key_account(
            "sk-plain-123456".to_string(),
            Some("https://plain.example/v1".to_string()),
            Some("Plain".to_string()),
            None,
            None,
        )
        .expect("add account without default model");
    store
        .switch_account(&plain.id)
        .expect("switch to account without default model");
    let plain_config =
        fs::read_to_string(codex.path().join("config.toml")).expect("read plain account config");
    let plain_document = plain_config
        .parse::<toml_edit::Document>()
        .expect("parse plain account config");
    assert!(plain_document.get("model").is_none());
    assert!(plain_document.get("model_reasoning_effort").is_none());

    let oauth = store
        .import_from_json(
            &json!({
                "email": "oauth-after-model@example.com",
                "tokens": {
                    "id_token": "id-token",
                    "access_token": "access-token",
                    "refresh_token": "refresh-token"
                }
            })
            .to_string(),
        )
        .expect("import oauth account")
        .remove(0);
    store
        .switch_account(&oauth.id)
        .expect("switch to oauth account");
    let oauth_config =
        fs::read_to_string(codex.path().join("config.toml")).expect("read oauth config");
    let oauth_document = oauth_config
        .parse::<toml_edit::Document>()
        .expect("parse oauth config");
    assert_eq!(oauth_document["model_provider"].as_str(), Some("openai"));
    assert!(oauth_document.get("model").is_none());
}

#[test]
fn ordinary_default_model_preserves_unmanaged_config_fields() {
    let (_storage, codex, store) = test_store();
    let account = store
        .add_api_key_account(
            "sk-ordinary-123456".to_string(),
            Some("https://relay.example/v1".to_string()),
            Some("Relay".to_string()),
            None,
            None,
        )
        .expect("add ordinary api key account");
    store.switch_account(&account.id).expect("switch account");

    fs::write(
        codex.path().join("config.toml"),
        r#"model_provider = "relay"
model_reasoning_effort = "medium"
disable_response_storage = false
network_access = "restricted"
windows_wsl_setup_acknowledged = false
requires_openai_auth = false
custom_setting = "keep"

[features]
goals = false
js_repl = true
memories = false
custom_flag = true

[projects."/tmp/project"]
trust_level = "trusted"

[model_providers.relay]
name = "Relay"
base_url = "https://relay.example/v1"
wire_api = "responses"
requires_openai_auth = true
experimental_bearer_token = "sk-ordinary-123456"
"#,
    )
    .expect("write existing config");

    store
        .update_api_key_default_model(&account.id, "gpt-5.5".to_string())
        .expect("save ordinary default model");
    let config = fs::read_to_string(codex.path().join("config.toml")).expect("read config");
    let document = config.parse::<toml_edit::Document>().expect("parse config");
    assert_eq!(document["model"].as_str(), Some("gpt-5.5"));
    assert_eq!(document["model_reasoning_effort"].as_str(), Some("medium"));
    assert_eq!(document["disable_response_storage"].as_bool(), Some(false));
    assert_eq!(document["network_access"].as_str(), Some("restricted"));
    assert_eq!(
        document["windows_wsl_setup_acknowledged"].as_bool(),
        Some(false)
    );
    assert_eq!(document["requires_openai_auth"].as_bool(), Some(false));
    assert_eq!(document["custom_setting"].as_str(), Some("keep"));
    assert_eq!(document["features"]["goals"].as_bool(), Some(false));
    assert_eq!(document["features"]["js_repl"].as_bool(), Some(true));
    assert_eq!(document["features"]["memories"].as_bool(), Some(false));
    assert_eq!(document["features"]["custom_flag"].as_bool(), Some(true));
    assert_eq!(
        document["projects"]["/tmp/project"]["trust_level"].as_str(),
        Some("trusted")
    );

    let oauth = store
        .import_from_json(
            &json!({
                "email": "oauth-after-ordinary@example.com",
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
    store
        .switch_account(&oauth.id)
        .expect("switch oauth account");
    let oauth_config =
        fs::read_to_string(codex.path().join("config.toml")).expect("read oauth config");
    let oauth_document = oauth_config
        .parse::<toml_edit::Document>()
        .expect("parse oauth config");
    assert!(oauth_document.get("model").is_none());
    assert_eq!(
        oauth_document["model_reasoning_effort"].as_str(),
        Some("medium")
    );
    assert_eq!(oauth_document["features"]["js_repl"].as_bool(), Some(true));
    assert_eq!(oauth_document["custom_setting"].as_str(), Some("keep"));
}

#[test]
fn gpt_5_6_transition_restores_original_managed_config_values() {
    let (_storage, codex, store) = test_store();
    let account = store
        .add_api_key_account(
            "sk-restore-123456".to_string(),
            Some("https://relay.example/v1".to_string()),
            Some("Relay".to_string()),
            None,
            None,
        )
        .expect("add api key account");
    store.switch_account(&account.id).expect("switch account");
    fs::write(
        codex.path().join("config.toml"),
        r#"model = "user-model"
model_provider = "relay"
model_reasoning_effort = "medium"
disable_response_storage = false
network_access = "restricted"
windows_wsl_setup_acknowledged = false
requires_openai_auth = false
supports_websockets = true
websocket_v2 = true

[features]
goals = false
js_repl = true
memories = false
supports_websockets = true
websocket_v2 = true

[model_providers.relay]
name = "Original Relay"
base_url = "https://relay.example/v1"
wire_api = "responses"
experimental_bearer_token = "sk-restore-123456"
supports_websockets = true
websocket_v2 = true
"#,
    )
    .expect("write original config");

    store
        .update_api_key_default_model(&account.id, "gpt-5.6-sol".to_string())
        .expect("enable gpt 5.6 model");
    let managed =
        fs::read_to_string(codex.path().join("config.toml")).expect("read managed config");
    let mut managed = managed
        .parse::<toml_edit::Document>()
        .expect("parse managed config");
    assert_eq!(managed["model"].as_str(), Some("gpt-5.6-sol"));
    assert_eq!(managed["model_reasoning_effort"].as_str(), Some("high"));
    assert!(managed.get("supports_websockets").is_none());
    assert!(managed["features"].get("websocket_v2").is_none());
    assert!(managed["model_providers"]["relay"]
        .get("supports_websockets")
        .is_none());
    managed["network_access"] = toml_edit::value("manual-during-managed");
    managed["features"]["goals"] = toml_edit::value("manual-during-managed");
    managed["model_providers"]["relay"]["supports_websockets"] = toml_edit::value(false);
    fs::write(codex.path().join("config.toml"), managed.to_string())
        .expect("write external managed-field edits");

    let oauth = store
        .import_from_json(
            &json!({
                "email": "restore-oauth@example.com",
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
    store.switch_account(&oauth.id).expect("switch oauth");

    let restored =
        fs::read_to_string(codex.path().join("config.toml")).expect("read restored config");
    let restored = restored
        .parse::<toml_edit::Document>()
        .expect("parse restored config");
    assert_eq!(restored["model"].as_str(), Some("user-model"));
    assert_eq!(restored["model_provider"].as_str(), Some("openai"));
    assert_eq!(restored["model_reasoning_effort"].as_str(), Some("medium"));
    assert_eq!(restored["disable_response_storage"].as_bool(), Some(false));
    assert_eq!(
        restored["network_access"].as_str(),
        Some("manual-during-managed")
    );
    assert_eq!(
        restored["windows_wsl_setup_acknowledged"].as_bool(),
        Some(false)
    );
    assert_eq!(restored["requires_openai_auth"].as_bool(), Some(false));
    assert_eq!(restored["supports_websockets"].as_bool(), Some(true));
    assert_eq!(restored["websocket_v2"].as_bool(), Some(true));
    assert_eq!(
        restored["features"]["goals"].as_str(),
        Some("manual-during-managed")
    );
    assert_eq!(restored["features"]["js_repl"].as_bool(), Some(true));
    assert_eq!(restored["features"]["memories"].as_bool(), Some(false));
    assert_eq!(
        restored["features"]["supports_websockets"].as_bool(),
        Some(true)
    );
    assert_eq!(restored["features"]["websocket_v2"].as_bool(), Some(true));
    assert_eq!(
        restored["model_providers"]["relay"]["supports_websockets"].as_bool(),
        Some(false)
    );
    assert_eq!(
        restored["model_providers"]["relay"]["websocket_v2"].as_bool(),
        Some(true)
    );
}

#[test]
fn switching_between_gpt_5_6_providers_restores_every_touched_provider() {
    let (_storage, codex, store) = test_store();
    let provider_a = store
        .add_api_key_account(
            "sk-provider-a-123456".to_string(),
            Some("https://a.example/v1".to_string()),
            Some("Provider A".to_string()),
            None,
            None,
        )
        .expect("add provider a");
    let provider_b = store
        .add_api_key_account(
            "sk-provider-b-123456".to_string(),
            Some("https://b.example/v1".to_string()),
            Some("Provider B".to_string()),
            None,
            None,
        )
        .expect("add provider b");
    for provider in [&provider_a, &provider_b] {
        let mut imported = serde_json::to_value(provider).expect("serialize provider fixture");
        imported["default_model"] = json!("gpt-5.6-sol");
        store
            .update_account_from_json(&provider.id, &imported.to_string())
            .expect("seed imported provider default model");
    }
    fs::write(
        codex.path().join("config.toml"),
        r#"model = "baseline-model"

[model_providers.provider_a]
supports_websockets = true
websocket_v2 = true

[model_providers.provider_b]
supports_websockets = false
websocket_v2 = true
"#,
    )
    .expect("write provider baseline");

    store
        .switch_account(&provider_a.id)
        .expect("switch provider a");
    store
        .switch_account(&provider_b.id)
        .expect("switch provider b");
    let both_managed =
        fs::read_to_string(codex.path().join("config.toml")).expect("read managed providers");
    let both_managed = both_managed
        .parse::<toml_edit::Document>()
        .expect("parse managed providers");
    for provider_id in ["provider_a", "provider_b"] {
        assert!(both_managed["model_providers"][provider_id]
            .get("supports_websockets")
            .is_none());
        assert!(both_managed["model_providers"][provider_id]
            .get("websocket_v2")
            .is_none());
    }

    let oauth = store
        .import_from_json(
            &json!({
                "email": "providers-oauth@example.com",
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
    store.switch_account(&oauth.id).expect("switch oauth");
    let restored =
        fs::read_to_string(codex.path().join("config.toml")).expect("read restored providers");
    let restored = restored
        .parse::<toml_edit::Document>()
        .expect("parse restored providers");
    assert_eq!(restored["model"].as_str(), Some("baseline-model"));
    assert_eq!(
        restored["model_providers"]["provider_a"]["supports_websockets"].as_bool(),
        Some(true)
    );
    assert_eq!(
        restored["model_providers"]["provider_a"]["websocket_v2"].as_bool(),
        Some(true)
    );
    assert_eq!(
        restored["model_providers"]["provider_b"]["supports_websockets"].as_bool(),
        Some(false)
    );
    assert_eq!(
        restored["model_providers"]["provider_b"]["websocket_v2"].as_bool(),
        Some(true)
    );
}

#[test]
fn setting_noncurrent_default_model_is_rejected_without_mutation() {
    let (storage, codex, store) = test_store();
    let provider_a = store
        .add_api_key_account(
            "sk-owner-a-123456".to_string(),
            Some("https://a.example/v1".to_string()),
            Some("Owner A".to_string()),
            None,
            None,
        )
        .expect("add owner a");
    let provider_b = store
        .add_api_key_account(
            "sk-owner-b-123456".to_string(),
            Some("https://b.example/v1".to_string()),
            Some("Owner B".to_string()),
            None,
            None,
        )
        .expect("add owner b");
    store
        .switch_account(&provider_a.id)
        .expect("switch owner a");
    fs::write(
        codex.path().join("config.toml"),
        r#"model = "baseline-model"
model_provider = "owner_a"
network_access = "restricted"

[model_providers.owner_a]
base_url = "https://a.example/v1"
experimental_bearer_token = "sk-owner-a-123456"
"#,
    )
    .expect("write owner baseline");
    store
        .update_api_key_default_model(&provider_a.id, "gpt-5.6-sol".to_string())
        .expect("set owner a model");

    let accounts_before =
        fs::read(storage.path().join("accounts.json")).expect("read accounts before rejection");
    let config_before =
        fs::read(codex.path().join("config.toml")).expect("read config before rejection");
    assert!(!store
        .check_api_key_model_access(&provider_b.id)
        .expect("check owner b access"));
    let error = store
        .update_api_key_default_model(&provider_b.id, "gpt-5.6-sol".to_string())
        .expect_err("reject non-current owner b model");
    assert!(error.contains("当前 Codex 配置不是该 API Key 账号"));
    assert_eq!(
        fs::read(storage.path().join("accounts.json")).expect("read accounts after rejection"),
        accounts_before
    );
    assert_eq!(
        fs::read(codex.path().join("config.toml")).expect("read config after rejection"),
        config_before
    );
    let stored_provider_b = store
        .list_accounts()
        .expect("list accounts")
        .into_iter()
        .find(|account| account.id == provider_b.id)
        .expect("find owner b");
    assert!(stored_provider_b.default_model.is_none());

    let oauth = store
        .import_from_json(
            &json!({
                "email": "owner-oauth@example.com",
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
    store.switch_account(&oauth.id).expect("switch oauth");

    let restored =
        fs::read_to_string(codex.path().join("config.toml")).expect("read restored config");
    let restored = restored
        .parse::<toml_edit::Document>()
        .expect("parse restored config");
    assert_eq!(restored["model"].as_str(), Some("baseline-model"));
    assert_eq!(restored["network_access"].as_str(), Some("restricted"));
}

#[test]
fn current_managed_model_json_edits_apply_transitions_immediately() {
    let (_storage, codex, store) = test_store();
    let account = store
        .add_api_key_account(
            "sk-json-transition-123456".to_string(),
            Some("https://json.example/v1".to_string()),
            Some("JSON Relay".to_string()),
            None,
            None,
        )
        .expect("add json transition account");
    store
        .switch_account(&account.id)
        .expect("switch json transition account");
    fs::write(
        codex.path().join("config.toml"),
        r#"model = "json-baseline-model"
model_provider = "json_relay"
model_reasoning_effort = "medium"
disable_response_storage = false
network_access = "restricted"
windows_wsl_setup_acknowledged = false
requires_openai_auth = false

[features]
goals = false
js_repl = true
memories = false

[model_providers.json_relay]
base_url = "https://json.example/v1"
experimental_bearer_token = "sk-json-transition-123456"
supports_websockets = true
websocket_v2 = true
"#,
    )
    .expect("write json transition baseline");
    store
        .update_api_key_default_model(&account.id, "gpt-5.6-sol".to_string())
        .expect("set managed json model");

    let current = store
        .list_accounts()
        .expect("list managed account")
        .into_iter()
        .find(|candidate| candidate.id == account.id)
        .expect("find managed account");
    let mut renamed_provider = serde_json::to_value(current).expect("serialize managed account");
    renamed_provider["api_provider_name"] = json!("JSON Relay Renamed");
    let renamed_provider = store
        .update_account_from_json(&account.id, &renamed_provider.to_string())
        .expect("rename managed provider");
    let renamed_config = fs::read_to_string(codex.path().join("config.toml"))
        .expect("read renamed provider config")
        .parse::<toml_edit::Document>()
        .expect("parse renamed provider config");
    assert_eq!(renamed_config["model"].as_str(), Some("gpt-5.6-sol"));
    assert!(renamed_config["model_providers"]["json_relay_renamed"]
        .get("supports_websockets")
        .is_none());

    let mut ordinary =
        serde_json::to_value(renamed_provider).expect("serialize renamed provider account");
    ordinary["default_model"] = json!("gpt-5.5");
    let ordinary = store
        .update_account_from_json(&account.id, &ordinary.to_string())
        .expect("change managed model to ordinary model");
    assert_eq!(ordinary.default_model.as_deref(), Some("gpt-5.5"));
    let ordinary_config = fs::read_to_string(codex.path().join("config.toml"))
        .expect("read ordinary model config")
        .parse::<toml_edit::Document>()
        .expect("parse ordinary model config");
    assert_eq!(ordinary_config["model"].as_str(), Some("gpt-5.5"));
    assert_eq!(
        ordinary_config["model_reasoning_effort"].as_str(),
        Some("medium")
    );
    assert_eq!(
        ordinary_config["disable_response_storage"].as_bool(),
        Some(false)
    );
    assert_eq!(
        ordinary_config["network_access"].as_str(),
        Some("restricted")
    );
    assert_eq!(ordinary_config["features"]["goals"].as_bool(), Some(false));
    assert_eq!(ordinary_config["features"]["js_repl"].as_bool(), Some(true));
    assert_eq!(
        ordinary_config["model_providers"]["json_relay"]["supports_websockets"].as_bool(),
        Some(true)
    );
    assert_eq!(
        ordinary_config["model_providers"]["json_relay"]["websocket_v2"].as_bool(),
        Some(true)
    );
    assert_eq!(
        ordinary_config["model_providers"]["json_relay_renamed"]["supports_websockets"].as_bool(),
        Some(false)
    );

    let mut cleared = serde_json::to_value(ordinary).expect("serialize ordinary model account");
    cleared["default_model"] = serde_json::Value::Null;
    let cleared = store
        .update_account_from_json(&account.id, &cleared.to_string())
        .expect("clear managed model");
    assert!(cleared.default_model.is_none());
    let cleared_config = fs::read_to_string(codex.path().join("config.toml"))
        .expect("read cleared model config")
        .parse::<toml_edit::Document>()
        .expect("parse cleared model config");
    assert_eq!(
        cleared_config["model"].as_str(),
        Some("json-baseline-model")
    );
    assert_eq!(
        cleared_config["network_access"].as_str(),
        Some("restricted")
    );
}

#[test]
fn renaming_current_managed_account_keeps_its_config_backup_owner() {
    let (_storage, codex, store) = test_store();
    let account = store
        .add_api_key_account(
            "sk-renamed-owner-123456".to_string(),
            Some("https://rename.example/v1".to_string()),
            Some("Rename Provider".to_string()),
            None,
            None,
        )
        .expect("add rename account");
    store
        .switch_account(&account.id)
        .expect("switch rename account");
    fs::write(
        codex.path().join("config.toml"),
        r#"model = "rename-baseline"
model_provider = "rename_provider"
network_access = "restricted"

[model_providers.rename_provider]
base_url = "https://rename.example/v1"
experimental_bearer_token = "sk-renamed-owner-123456"
"#,
    )
    .expect("write rename baseline");
    store
        .update_api_key_default_model(&account.id, "gpt-5.6-sol".to_string())
        .expect("set rename model");

    let current_account = store
        .list_accounts()
        .expect("list accounts")
        .into_iter()
        .find(|candidate| candidate.id == account.id)
        .expect("find rename account");
    let mut editable = serde_json::to_value(current_account).expect("serialize editable account");
    editable["id"] = json!("renamed-managed-account");
    let renamed = store
        .update_account_from_json(&account.id, &editable.to_string())
        .expect("rename current managed account");
    assert_eq!(renamed.id, "renamed-managed-account");

    let oauth = store
        .import_from_json(
            &json!({
                "email": "renamed-owner-oauth@example.com",
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
    store.switch_account(&oauth.id).expect("switch oauth");

    let restored =
        fs::read_to_string(codex.path().join("config.toml")).expect("read renamed restoration");
    let restored = restored
        .parse::<toml_edit::Document>()
        .expect("parse renamed restoration");
    assert_eq!(restored["model"].as_str(), Some("rename-baseline"));
    assert_eq!(restored["network_access"].as_str(), Some("restricted"));
}

#[test]
fn detecting_an_external_account_switch_releases_stale_managed_backup() {
    let (_storage, codex, store) = test_store();
    let provider_a = store
        .add_api_key_account(
            "sk-detect-a-123456".to_string(),
            Some("https://a.example/v1".to_string()),
            Some("Detect A".to_string()),
            None,
            None,
        )
        .expect("add detect a");
    let provider_b = store
        .add_api_key_account(
            "sk-detect-b-123456".to_string(),
            Some("https://b.example/v1".to_string()),
            Some("Detect B".to_string()),
            None,
            None,
        )
        .expect("add detect b");
    store
        .switch_account(&provider_a.id)
        .expect("switch detect a");
    fs::write(
        codex.path().join("config.toml"),
        r#"model = "baseline-model"
model_provider = "detect_a"

[model_providers.detect_a]
base_url = "https://a.example/v1"
experimental_bearer_token = "sk-detect-a-123456"
"#,
    )
    .expect("write baseline");
    store
        .update_api_key_default_model(&provider_a.id, "gpt-5.6-sol".to_string())
        .expect("set detect a model");

    fs::write(
        codex.path().join("config.toml"),
        r#"model = "external-model"
model_provider = "detect_b"

[model_providers.detect_b]
base_url = "https://b.example/v1"
experimental_bearer_token = "sk-detect-b-123456"
"#,
    )
    .expect("write external provider config");
    let detected = store
        .detect_current_account_from_codex_config()
        .expect("detect external account")
        .expect("detected account");
    assert_eq!(detected.id, provider_b.id);

    let oauth = store
        .import_from_json(
            &json!({
                "email": "detect-oauth@example.com",
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
    store.switch_account(&oauth.id).expect("switch oauth");
    let config = fs::read_to_string(codex.path().join("config.toml")).expect("read final config");
    let config = config
        .parse::<toml_edit::Document>()
        .expect("parse final config");
    assert_eq!(config["model"].as_str(), Some("external-model"));
}

#[test]
fn manual_config_edit_releases_managed_default_model_state() {
    let (_storage, codex, store) = test_store();
    let account = store
        .add_api_key_account(
            "sk-manual-123456".to_string(),
            Some("https://relay.example/v1".to_string()),
            Some("Relay".to_string()),
            None,
            None,
        )
        .expect("add api key account");
    store.switch_account(&account.id).expect("switch account");
    store
        .update_api_key_default_model(&account.id, "gpt-5.6-sol".to_string())
        .expect("set managed model");

    fs::write(
        codex.path().join("config.toml"),
        "model_provider = \"relay\"\nmodel = \"manual-model\"\nnetwork_access = \"manual\"\n",
    )
    .expect("write manual config");
    assert!(store
        .release_current_api_key_default_model()
        .expect("release managed model"));
    let released = store
        .list_accounts()
        .expect("list accounts")
        .into_iter()
        .find(|candidate| candidate.id == account.id)
        .expect("released account");
    assert!(released.default_model.is_none());

    let oauth = store
        .import_from_json(
            &json!({
                "email": "manual-oauth@example.com",
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
    store.switch_account(&oauth.id).expect("switch oauth");
    let config = fs::read_to_string(codex.path().join("config.toml")).expect("read manual config");
    let config = config
        .parse::<toml_edit::Document>()
        .expect("parse manual config");
    assert_eq!(config["model"].as_str(), Some("manual-model"));
    assert_eq!(config["network_access"].as_str(), Some("manual"));
}

#[test]
fn model_name_that_only_shares_gpt_5_6_prefix_does_not_enable_compatibility_config() {
    let (_storage, codex, store) = test_store();
    let account = store
        .add_api_key_account(
            "sk-gpt560-123456".to_string(),
            Some("https://relay.example/v1".to_string()),
            Some("Relay".to_string()),
            None,
            None,
        )
        .expect("add api key account");
    store.switch_account(&account.id).expect("switch account");
    store
        .update_api_key_default_model(&account.id, "gpt-5.60".to_string())
        .expect("save default model");

    let config = fs::read_to_string(codex.path().join("config.toml")).expect("read config");
    let document = config.parse::<toml_edit::Document>().expect("parse config");
    assert_eq!(document["model"].as_str(), Some("gpt-5.60"));
    assert!(document.get("model_reasoning_effort").is_none());
    assert_eq!(
        document["model_providers"]["relay"]["supports_websockets"].as_bool(),
        Some(false)
    );
}

#[test]
fn switching_directly_from_gpt_5_6_api_key_to_oauth_clears_managed_model_config() {
    let (_storage, codex, store) = test_store();
    let api = store
        .add_api_key_account(
            "sk-gpt56-oauth-123456".to_string(),
            Some("https://relay.example/v1".to_string()),
            Some("Relay".to_string()),
            None,
            None,
        )
        .expect("add api key account");
    store.switch_account(&api.id).expect("switch api account");
    store
        .update_api_key_default_model(&api.id, "gpt-5.6-sol".to_string())
        .expect("save gpt 5.6 default");

    let oauth = store
        .import_from_json(
            &json!({
                "email": "oauth-cleanup@example.com",
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
    store
        .switch_account(&oauth.id)
        .expect("switch oauth account");

    let config = fs::read_to_string(codex.path().join("config.toml")).expect("read oauth config");
    let document = config
        .parse::<toml_edit::Document>()
        .expect("parse oauth config");
    assert_eq!(document["model_provider"].as_str(), Some("openai"));
    for key in [
        "model",
        "model_reasoning_effort",
        "disable_response_storage",
        "network_access",
        "windows_wsl_setup_acknowledged",
        "requires_openai_auth",
    ] {
        assert!(document.get(key).is_none(), "{key} should be removed");
    }
    for key in ["goals", "js_repl", "memories"] {
        assert!(
            document
                .get("features")
                .and_then(toml_edit::Item::as_table_like)
                .and_then(|features| features.get(key))
                .is_none(),
            "features.{key} should be removed"
        );
    }
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
        assert!(start_args.iter().any(|arg| arg.contains("OpenAI.Codex_*")));
        assert!(start_args
            .iter()
            .all(|arg| !arg.contains("-Filter 'Codex*.lnk'")));
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        assert_eq!(start_program, "codex");
    }
}
