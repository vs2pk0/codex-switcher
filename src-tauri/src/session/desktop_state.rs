use super::*;
use serde_json::Map;

const RENDERER_STATE: &str = "electron-account-renderer-state-v1";
#[cfg(test)]
const ATOMS: &str = "electron-persisted-atom-state";
const LOCAL_PROJECT_KEYS: &[&str] = &[
    "local-projects",
    "project-order",
    "thread-project-assignments",
    "thread-project-membership-host-ids",
    "sidebar-project-thread-orders",
    "app-server-project-id-by-legacy-project-id-by-host",
    "app-server-projects-migration-by-host",
];

impl SessionStore {
    /// Run while the instance is stopped, after writing its destination auth.json.
    /// The desktop archives globals on startup and restores the selected account's
    /// snapshot. Updating only the top-level projects is therefore insufficient.
    pub(crate) fn preserve_account_scoped_local_projects(&self) -> Result<(), String> {
        let path = self.codex_home.join(".codex-global-state.json");
        if !path.is_file() {
            return Ok(());
        }
        let original = fs::read(&path).map_err(|e| format!("读取账号项目状态失败：{e}"))?;
        let mut state: Value =
            serde_json::from_slice(&original).map_err(|e| format!("解析账号项目状态失败：{e}"))?;
        let before = state.clone();
        self.merge_account_scoped_local_projects(&mut state)?;
        if state == before {
            return Ok(());
        }
        let bytes = serde_json::to_vec(&state).map_err(|e| e.to_string())?;
        // Keep the original independently of the desktop's rolling .bak file.
        let backup = path.with_extension(format!(
            "json.switcher-projects-{:016x}.bak",
            rand::random::<u64>()
        ));
        write_bytes_atomic(&backup, &original)?;
        write_bytes_atomic(&path.with_extension("json.bak"), &bytes)?;
        write_bytes_atomic(&path, &bytes)
    }

    pub(super) fn merge_account_scoped_local_projects(
        &self,
        state: &mut Value,
    ) -> Result<(), String> {
        let Some(renderer) = state.get(RENDERER_STATE) else {
            return Ok(()); // Older desktops have no account-scoped renderer state.
        };
        let inactive = renderer
            .get("inactive")
            .and_then(Value::as_object)
            .ok_or_else(|| "账号侧栏快照格式无效，已保留原状态".to_string())?;
        if !renderer
            .get("owner")
            .is_some_and(|v| v.is_null() || v.is_string())
        {
            return Err("账号侧栏归属格式无效，已保留原状态".to_string());
        }
        let Some(owner) = self.renderer_owner_from_auth()? else {
            return Ok(()); // Unbound API Key accounts have no renderer identity.
        };
        let mut shared = local_project_globals(state);
        for snapshot in inactive.values() {
            if let Some(globals) = snapshot.get("globals") {
                merge_project_globals(&mut shared, &local_project_globals(globals));
            }
        }
        if shared.as_object().is_none_or(Map::is_empty) {
            return Ok(());
        }

        if renderer.get("owner").and_then(Value::as_str) != Some(owner.as_str()) {
            // Seed a new account, or extend its existing snapshot. Keep its own
            // atoms and unrelated globals: cloud caches and OAuth state are private.
            let snapshot = state[RENDERER_STATE]["inactive"]
                .as_object_mut()
                .unwrap()
                .entry(owner)
                .or_insert_with(|| serde_json::json!({"atoms": {}, "globals": {}}));
            if !snapshot.get("atoms").is_some_and(Value::is_object)
                || !snapshot.get("globals").is_some_and(Value::is_object)
            {
                return Err("目标账号侧栏快照格式无效，已保留原状态".to_string());
            }
            merge_project_globals(&mut snapshot["globals"], &shared);
        }
        // Also recover projects when the account was already switched by an older
        // Switcher version. Do not change owner; the desktop still switches normally.
        merge_project_globals(state, &shared);
        Ok(())
    }

    fn renderer_owner_from_auth(&self) -> Result<Option<String>, String> {
        let path = self.codex_home.join("auth.json");
        if !path.is_file() {
            return Ok(None);
        }
        let auth: Value = serde_json::from_slice(
            &fs::read(path).map_err(|e| format!("读取当前账号身份失败：{e}"))?,
        )
        .map_err(|e| format!("解析当前账号身份失败：{e}"))?;
        let Some(tokens) = auth.get("tokens").and_then(Value::as_object) else {
            return Ok(None);
        };
        // The desktop derives its principal from the access token, preferring
        // user_id to chatgpt_user_id. Keep each identity within one token so stale
        // ID-token claims cannot shadow or combine with the active principal.
        let identity = ["access_token", "id_token"].iter().find_map(|name| {
            let token = tokens.get(*name)?.as_str()?;
            let claim = |key| crate::account::jwt_auth_claim_string(token, key);
            let user = claim("user_id").or_else(|| claim("chatgpt_user_id"))?;
            let account = claim("chatgpt_account_id")
                .or_else(|| claim("account_id"))
                .or_else(|| {
                    tokens
                        .get("account_id")?
                        .as_str()
                        .filter(|id| !id.trim().is_empty())
                        .map(str::to_string)
                })?;
            Some([user, account])
        });
        match identity {
            Some(identity) => {
                // Match the desktop's SHA256(JSON.stringify([userId, accountId])).
                let identity = serde_json::to_vec(&identity).map_err(|e| e.to_string())?;
                Ok(Some(format!("{:x}", Sha256::digest(identity))))
            }
            _ => Err("当前 OAuth 凭据缺少用户或工作区身份，无法同步账号侧栏".to_string()),
        }
    }
}

/// Only local Codex project metadata crosses accounts. Never copy renderer atoms,
/// selected cloud projects, remote host mappings, or account-specific credentials.
fn local_project_globals(state: &Value) -> Value {
    let mut result = Map::new();
    let is_local_project = |id: &str| {
        state
            .get("local-projects")
            .and_then(Value::as_object)
            .is_some_and(|projects| projects.contains_key(id))
    };
    for &key in LOCAL_PROJECT_KEYS {
        let Some(value) = state.get(key) else {
            continue;
        };
        let value = match key {
            "project-order" => Value::Array(
                value
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|id| id.as_str().is_some_and(is_local_project))
                    .cloned()
                    .collect(),
            ),
            "sidebar-project-thread-orders" => filter_map(value, |id, _| is_local_project(id)),
            "thread-project-assignments" => filter_map(value, |_, v| {
                v.get("projectKind").and_then(Value::as_str) == Some("local")
            }),
            "thread-project-membership-host-ids" => {
                filter_map(value, |_, v| v.as_str() == Some("local"))
            }
            "app-server-project-id-by-legacy-project-id-by-host"
            | "app-server-projects-migration-by-host" => {
                filter_map(value, |host, _| host.starts_with("local:"))
            }
            _ => value.clone(),
        };
        result.insert(key.to_string(), value);
    }
    Value::Object(result)
}

fn filter_map(value: &Value, include: impl Fn(&str, &Value) -> bool) -> Value {
    Value::Object(
        value
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(key, value)| include(key, value))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    )
}

/// Merge collections, but retain complete destination records. Recursing into
/// project records would restore removed rootPaths or stale optional fields.
fn merge_project_globals(destination: &mut Value, source: &Value) {
    let (Some(destination), Some(source)) = (destination.as_object_mut(), source.as_object())
    else {
        return;
    };
    for (key, value) in source {
        let depth = match key.as_str() {
            "project-order" => 0,
            "app-server-project-id-by-legacy-project-id-by-host" => 2,
            _ => 1,
        };
        match destination.get_mut(key) {
            Some(existing) => merge_missing(existing, value, depth),
            None => {
                destination.insert(key.clone(), value.clone());
            }
        }
    }
}

/// Destination values win; only collection maps and ordered lists are merged.
fn merge_missing(destination: &mut Value, source: &Value, map_depth: usize) {
    match (destination, source) {
        (Value::Object(destination), Value::Object(source)) if map_depth > 0 => {
            for (key, value) in source {
                match destination.get_mut(key) {
                    Some(existing) => merge_missing(existing, value, map_depth - 1),
                    None => {
                        destination.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (Value::Array(destination), Value::Array(source)) if map_depth == 0 => {
            for value in source {
                if !destination.contains(value) {
                    destination.push(value.clone());
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use serde_json::json;
    use tempfile::tempdir;

    fn write_auth(store: &SessionStore, user: &str, account: &str) -> String {
        let token = format!(
            "e30.{}.signature",
            URL_SAFE_NO_PAD.encode(
                json!({
                    "https://api.openai.com/auth": {
                        "chatgpt_user_id": user
                    }
                })
                .to_string()
            )
        );
        fs::write(
            store.codex_home.join("auth.json"),
            json!({
                "tokens": {"id_token": token, "account_id": account}
            })
            .to_string(),
        )
        .unwrap();
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&[user, account]).unwrap())
        )
    }

    #[test]
    fn sidebar_owner_matches_access_token_principal_when_id_claims_differ() {
        let home = tempdir().unwrap();
        let store = SessionStore::new(home.path().into());
        let token = |claims: Value| {
            format!(
                "e30.{}.signature",
                URL_SAFE_NO_PAD.encode(json!({"https://api.openai.com/auth": claims}).to_string())
            )
        };
        for account_claim in ["chatgpt_account_id", "account_id"] {
            fs::write(
                store.codex_home.join("auth.json"),
                json!({"tokens": {
                    "account_id": "stale-workspace",
                    "id_token": token(json!({
                        "user_id": "stale-user", "chatgpt_account_id": "stale-workspace"
                    })),
                    "access_token": token(json!({
                        "user_id": "active-user", "chatgpt_user_id": "chatgpt-user",
                        account_claim: "active-workspace"
                    }))
                }})
                .to_string(),
            )
            .unwrap();
            let owner = format!(
                "{:x}",
                Sha256::digest(br#"["active-user","active-workspace"]"#)
            );
            let mut state = json!({
                RENDERER_STATE: {"owner": "old", "inactive": {}},
                "local-projects": {"saved": {"name": "Saved project"}}
            });
            store
                .merge_account_scoped_local_projects(&mut state)
                .unwrap();
            select_owner(&mut state, None);
            select_owner(&mut state, Some(&owner));
            assert_eq!(state["local-projects"]["saved"]["name"], "Saved project");
        }
    }

    #[test]
    fn sidebar_owner_does_not_combine_claims_from_different_tokens() {
        let home = tempdir().unwrap();
        let store = SessionStore::new(home.path().into());
        let token = |claims: Value| {
            format!(
                "e30.{}.signature",
                URL_SAFE_NO_PAD.encode(json!({"https://api.openai.com/auth": claims}).to_string())
            )
        };
        fs::write(
            store.codex_home.join("auth.json"),
            json!({"tokens": {
                "id_token": token(json!({"chatgpt_account_id": "old-workspace"})),
                "access_token": token(json!({"user_id": "active-user"}))
            }})
            .to_string(),
        )
        .unwrap();
        assert!(store.renderer_owner_from_auth().is_err());
    }

    // Model the desktop's startup (owner -> null) and subsequent account selection.
    fn select_owner(state: &mut Value, owner: Option<&str>) {
        if owner.is_some() && state[RENDERER_STATE]["owner"].as_str() == owner {
            return;
        }
        let previous = state[RENDERER_STATE]["owner"].as_str().map(str::to_string);
        let mut globals = Map::new();
        for &key in LOCAL_PROJECT_KEYS.iter().chain(["selected-project"].iter()) {
            if let Some(value) = state.as_object_mut().unwrap().remove(key) {
                globals.insert(key.to_string(), value);
            }
        }
        let atoms = state
            .as_object_mut()
            .unwrap()
            .remove(ATOMS)
            .unwrap_or(json!({}));
        let inactive = state[RENDERER_STATE]["inactive"].as_object_mut().unwrap();
        if let Some(previous) = previous {
            inactive.insert(previous, json!({"atoms": atoms, "globals": globals}));
        }
        let selected = owner.and_then(|owner| inactive.remove(owner));
        state[RENDERER_STATE]["owner"] = json!(owner);
        state[ATOMS] = selected
            .as_ref()
            .map(|s| s["atoms"].clone())
            .unwrap_or(json!({}));
        if let Some(selected) = selected {
            for (key, value) in selected["globals"].as_object().unwrap() {
                state[key] = value.clone();
            }
        }
    }

    #[test]
    fn new_account_keeps_local_projects_after_desktop_startup_and_round_trip() {
        let home = tempdir().unwrap();
        let store = SessionStore::new(home.path().to_path_buf());
        let target = write_auth(&store, "new-user", "new-account");
        let mut state = json!({
            RENDERER_STATE: {"owner": "old-owner", "inactive": {}},
            ATOMS: {"oauth-cache": "old-private-data"},
            "local-projects": {"old-project": {"name": "Custom project", "rootPaths": ["/repo"]}},
            "project-order": ["old-project", "remote-project"],
            "sidebar-project-thread-orders": {
                "old-project": ["thread"], "remote-project": ["remote-thread"]
            },
            "thread-project-assignments": {
                "thread": {"projectKind": "local", "projectId": "old-project"},
                "remote-thread": {"projectKind": "remote", "projectId": "remote-project"}
            },
            "thread-project-membership-host-ids": {"thread": "local", "remote-thread": "ssh:host"},
            "app-server-project-id-by-legacy-project-id-by-host": {
                "local:/codex": {"old-project": "native-project"}, "ssh:host": {"x": "private"}
            },
            "selected-project": {"type": "cloud", "projectId": "private-cloud-project"},
            "unrelated-setting": true
        });
        let original = state.clone();
        store
            .merge_account_scoped_local_projects(&mut state)
            .unwrap();
        assert_eq!(state[RENDERER_STATE]["owner"], "old-owner");
        select_owner(&mut state, None);
        select_owner(&mut state, Some(&target));
        assert_eq!(state["local-projects"], original["local-projects"]);
        assert_eq!(state["project-order"], json!(["old-project"]));
        assert_eq!(
            state["sidebar-project-thread-orders"],
            json!({"old-project": ["thread"]})
        );
        assert_eq!(
            state["thread-project-assignments"]["thread"]["projectId"],
            "old-project"
        );
        assert!(state["thread-project-assignments"]
            .get("remote-thread")
            .is_none());
        assert!(state["thread-project-membership-host-ids"]
            .get("remote-thread")
            .is_none());
        assert!(state["app-server-project-id-by-legacy-project-id-by-host"]
            .get("ssh:host")
            .is_none());
        assert_eq!(state[ATOMS], json!({}));
        assert!(state.get("selected-project").is_none());
        assert_eq!(state["unrelated-setting"], true);
        assert_eq!(
            state[RENDERER_STATE]["inactive"]["old-owner"]["atoms"],
            original[ATOMS]
        );

        select_owner(&mut state, None);
        select_owner(&mut state, Some("old-owner"));
        assert_eq!(state[ATOMS], original[ATOMS]);
        assert_eq!(state["local-projects"], original["local-projects"]);
    }

    #[test]
    fn existing_destination_keeps_its_names_order_atoms_and_cloud_state() {
        let home = tempdir().unwrap();
        let store = SessionStore::new(home.path().to_path_buf());
        let target = write_auth(&store, "user", "workspace");
        let mut state = json!({
            RENDERER_STATE: {"owner": "old", "inactive": {
                target.clone(): {"atoms": {"private": "target"}, "globals": {
                    "local-projects": {"shared": {"name": "Target name", "rootPaths": ["/new-root"]}, "target-only": {"name": "Target"}},
                    "project-order": ["target-only", "shared"],
                    "sidebar-project-thread-orders": {"shared": ["new-thread", "same-thread"]},
                    "app-server-project-id-by-legacy-project-id-by-host": {
                        "local:/codex": {"shared": "target-native"}
                    },
                    "selected-project": {"type": "cloud", "projectId": "target-cloud"}
                }}
            }},
            "local-projects": {"shared": {"name": "Source name", "rootPaths": ["/old-root"], "stale": true}, "source-only": {"name": "Source"}},
            "project-order": ["shared", "source-only"],
            "sidebar-project-thread-orders": {"shared": ["same-thread", "old-thread"]},
            "app-server-project-id-by-legacy-project-id-by-host": {
                "local:/codex": {"shared": "old-native", "source-only": "source-native"}
            }
        });
        store
            .merge_account_scoped_local_projects(&mut state)
            .unwrap();
        let before = state.clone();
        store
            .merge_account_scoped_local_projects(&mut state)
            .unwrap();
        assert_eq!(state, before, "repeated preparation is idempotent");
        select_owner(&mut state, None);
        select_owner(&mut state, Some(&target));
        assert_eq!(
            state["local-projects"]["shared"],
            json!({
                "name": "Target name", "rootPaths": ["/new-root"]
            })
        );
        assert_eq!(state["local-projects"].as_object().unwrap().len(), 3);
        assert_eq!(
            state["project-order"],
            json!(["target-only", "shared", "source-only"])
        );
        assert_eq!(state[ATOMS], json!({"private": "target"}));
        assert_eq!(state["selected-project"]["projectId"], "target-cloud");
        assert_eq!(
            state["sidebar-project-thread-orders"]["shared"],
            json!(["new-thread", "same-thread", "old-thread"])
        );
        assert_eq!(
            state["app-server-project-id-by-legacy-project-id-by-host"]["local:/codex"],
            json!({
                "shared": "target-native", "source-only": "source-native"
            })
        );
    }

    #[test]
    fn one_click_repair_recovers_sidebar_without_rollout_files() {
        let home = tempdir().unwrap();
        let store = SessionStore::new(home.path().to_path_buf());
        let owner = write_auth(&store, "user", "account");
        let projects = json!({"saved": {"name": "Saved project", "rootPaths": ["/missing-root"]}});
        let state = json!({
            RENDERER_STATE: {"owner": owner, "inactive": {
                "old": {"atoms": {}, "globals": {
                    "local-projects": projects, "project-order": ["saved"]
                }}
            }}
        });
        let path = home.path().join(".codex-global-state.json");
        fs::write(&path, state.to_string()).unwrap();

        let outcome = crate::one_click_repair_session_store(&store, &mut |_| {}).unwrap();
        assert_eq!(outcome.session_count, 0);
        assert!(outcome.warnings.is_empty(), "{:?}", outcome.warnings);
        let repaired: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(repaired["local-projects"], projects);
        assert_eq!(repaired["project-order"], json!(["saved"]));
    }

    #[test]
    fn repair_keeps_saved_assignment_when_account_projects_share_a_root() {
        let home = tempdir().unwrap();
        let store = SessionStore::new(home.path().to_path_buf());
        let owner = write_auth(&store, "user", "account");
        let path = home.path().join(".codex-global-state.json");
        let saved_assignment = json!({"projectKind": "local", "projectId": "z-old"});
        fs::write(&path, json!({
            RENDERER_STATE: {"owner": owner, "inactive": {
                "old": {"atoms": {}, "globals": {
                    "local-projects": {"z-old": {"name": "Original", "rootPaths": [home.path()]}},
                    "thread-project-assignments": {"saved-thread": saved_assignment}
                }}
            }},
            "local-projects": {"a-current": {"name": "Current", "rootPaths": [home.path()]}}
        }).to_string()).unwrap();
        let rollout = home.path().join("saved-thread.jsonl");
        fs::write(
            &rollout,
            json!({
                "type": "session_meta", "payload": {
                    "id": "saved-thread", "cwd": home.path(), "source": "vscode"
                }
            })
            .to_string(),
        )
        .unwrap();

        let result = store
            .repair_desktop_projects(&[SessionRepairRecord {
                id: "saved-thread".to_string(),
                title: "Saved".to_string(),
                path: rollout,
                updated_at: 1,
            }])
            .unwrap();
        assert_eq!(result.created, 0);
        assert_eq!(result.updated_assignments, 0);
        assert_eq!(result.verified_assignments, 1);
        let repaired: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(repaired["local-projects"].as_object().unwrap().len(), 2);
        assert_eq!(
            repaired["thread-project-assignments"]["saved-thread"],
            saved_assignment
        );
        assert!(repaired["project-order"]
            .as_array()
            .unwrap()
            .contains(&json!("z-old")));
    }

    #[test]
    fn already_switched_account_recovers_inactive_projects_with_original_backup() {
        let home = tempdir().unwrap();
        let store = SessionStore::new(home.path().to_path_buf());
        let owner = write_auth(&store, "user", "account");
        let state = json!({
            RENDERER_STATE: {"owner": owner, "inactive": {
                "old": {"atoms": {"private": "old"}, "globals": {
                    "local-projects": {"old": {"name": "Original name"}}, "project-order": ["old"]
                }}
            }},
            ATOMS: {"private": "current"},
            "local-projects": {"current": {"name": "Current"}}, "project-order": ["current"]
        });
        let path = home.path().join(".codex-global-state.json");
        let original = serde_json::to_vec(&state).unwrap();
        fs::write(&path, &original).unwrap();
        store.preserve_account_scoped_local_projects().unwrap();
        let repaired: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(repaired["local-projects"].as_object().unwrap().len(), 2);
        assert_eq!(repaired[ATOMS], state[ATOMS]);
        assert_eq!(repaired[RENDERER_STATE], state[RENDERER_STATE]);
        assert_eq!(
            fs::read(path.with_extension("json.bak")).unwrap(),
            fs::read(&path).unwrap()
        );
        let backups: Vec<_> = fs::read_dir(home.path())
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .contains("switcher-projects-")
            })
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read(backups[0].path()).unwrap(), original);
        store.preserve_account_scoped_local_projects().unwrap();
        assert_eq!(
            fs::read(&path).unwrap(),
            serde_json::to_vec(&repaired).unwrap()
        );
    }

    #[test]
    fn legacy_and_unbound_api_states_stay_unchanged_and_invalid_states_fail_safely() {
        let home = tempdir().unwrap();
        let store = SessionStore::new(home.path().to_path_buf());
        for mut state in [
            json!({"local-projects": {}}),
            json!({
                RENDERER_STATE: {"owner": "old", "inactive": {}}, "local-projects": {"p": {}}
            }),
        ] {
            let original = state.clone();
            store
                .merge_account_scoped_local_projects(&mut state)
                .unwrap();
            assert_eq!(state, original);
        }
        let target = write_auth(&store, "user", "account");
        let mut state = json!({
            RENDERER_STATE: {"owner": "old", "inactive": {target: {"atoms": [], "globals": {}}}},
            "local-projects": {"p": {}}
        });
        let original = state.clone();
        assert!(store
            .merge_account_scoped_local_projects(&mut state)
            .is_err());
        assert_eq!(state, original);
    }
}
