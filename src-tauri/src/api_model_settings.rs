//! Optional API-account defaults. Keep ownership so switching back can restore user values.
use crate::account::write_bytes_atomic;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};
use toml_edit::{value, Document, Item};

const STATE_FILE: &str = "switcher-api-settings.json";
const KEYS: [&str; 5] = [
    "model_auto_compact_token_limit",
    "model_reasoning_effort",
    "web_search",
    "service_tier",
    "features.fast_mode",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct ApiModelSettings {
    pub auto_compact_enabled: bool,
    pub auto_compact_token_limit: u64,
    pub reasoning_enabled: bool,
    pub reasoning_levels: Vec<String>,
    pub reasoning_effort: String,
    pub disable_web_search: bool,
    pub fast_mode: bool,
}

impl Default for ApiModelSettings {
    fn default() -> Self {
        Self {
            auto_compact_enabled: false,
            auto_compact_token_limit: 180_000,
            reasoning_enabled: false,
            reasoning_levels: ["low", "medium", "high", "xhigh"]
                .map(str::to_owned)
                .to_vec(),
            reasoning_effort: "high".into(),
            disable_web_search: false,
            fast_mode: false,
        }
    }
}

impl ApiModelSettings {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.auto_compact_enabled
            && !(1..=2_147_483_647).contains(&self.auto_compact_token_limit)
        {
            return Err("自动压缩阈值必须为 1 到 2147483647 之间的整数".into());
        }
        if self.reasoning_enabled
            && (self.reasoning_levels.is_empty()
                || self.reasoning_levels.len() > 16
                || self
                    .reasoning_levels
                    .iter()
                    .any(|level| !valid_effort(level))
                || !self.reasoning_levels.contains(&self.reasoning_effort)
                || self
                    .reasoning_levels
                    .iter()
                    .collect::<std::collections::HashSet<_>>()
                    .len()
                    != self.reasoning_levels.len())
        {
            return Err("请选择有效且不重复的思考档位，并将默认强度设为其中一档".into());
        }
        Ok(())
    }
}

pub(crate) fn valid_effort(level: &str) -> bool {
    !level.is_empty()
        && level.len() <= 32
        && level
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct OverrideState(BTreeMap<String, OverrideValue>);

#[derive(Debug, Serialize, Deserialize)]
struct OverrideValue {
    previous: Option<String>,
    applied: Option<String>,
}

fn read_state(home: &Path) -> Result<OverrideState, String> {
    match std::fs::read(home.join(STATE_FILE)) {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).map_err(|e| format!("读取 API 设置备份失败：{e}"))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(OverrideState::default()),
        Err(e) => Err(format!("读取 API 设置备份失败：{e}")),
    }
}

fn raw(document: &Document, key: &str) -> Option<String> {
    if key == "features.fast_mode" {
        document
            .get("features")?
            .get("fast_mode")
            .map(ToString::to_string)
    } else {
        document.get(key).map(ToString::to_string)
    }
}

fn set_item(document: &mut Document, key: &str, next: Item) -> Result<(), String> {
    if key == "features.fast_mode" {
        if next.is_none() {
            if let Some(features) = document
                .get_mut("features")
                .and_then(Item::as_table_like_mut)
            {
                features.remove("fast_mode");
            }
        } else {
            document
                .as_table_mut()
                .entry("features")
                .or_insert(Item::Table(toml_edit::Table::new()))
                .as_table_like_mut()
                .ok_or("无法启用 Fast：config.toml 中的 features 必须为 TOML 表")?
                .insert("fast_mode", next);
        }
    } else if next.is_none() {
        document.remove(key);
    } else {
        document[key] = next;
    }
    Ok(())
}

/// Restore only values still owned by us; later manual edits always win.
pub(crate) fn restore_document(home: &Path, document: &mut Document) -> Result<(), String> {
    for (key, saved) in read_state(home)?.0 {
        if !KEYS.contains(&key.as_str())
            || raw(document, &key).as_deref().map(str::trim)
                != saved.applied.as_deref().map(str::trim)
        {
            continue;
        }
        if let Some(previous) = saved.previous {
            let mut parsed = format!("saved = {previous}\n")
                .parse::<Document>()
                .map_err(|e| e.to_string())?;
            let item = parsed
                .as_table_mut()
                .remove("saved")
                .ok_or("API 设置备份项为空")?;
            set_item(document, &key, item)?;
        } else {
            set_item(document, &key, Item::None)?;
        }
    }
    Ok(())
}

pub(crate) fn clear_state(home: &Path) -> Result<(), String> {
    if home.join(STATE_FILE).exists() {
        write_bytes_atomic(&home.join(STATE_FILE), b"{}")?;
    }
    Ok(())
}

/// Write config and its ownership record together, rolling the record back on failure.
pub(crate) fn write_config(
    home: &Path,
    document: &mut Document,
    settings: &ApiModelSettings,
    effort: Option<&str>,
) -> Result<(), String> {
    settings.validate()?;
    let mut state = OverrideState::default();
    let mut apply = |key: &str, next: Item| -> Result<(), String> {
        let previous = raw(document, key);
        set_item(document, key, next)?;
        state.0.insert(
            key.into(),
            OverrideValue {
                previous,
                applied: raw(document, key),
            },
        );
        Ok(())
    };
    if settings.auto_compact_enabled {
        apply(
            "model_auto_compact_token_limit",
            value(settings.auto_compact_token_limit as i64),
        )?;
    }
    if settings.reasoning_enabled {
        apply(
            "model_reasoning_effort",
            effort.map(value).unwrap_or(Item::None),
        )?;
    }
    if settings.disable_web_search {
        apply("web_search", value("disabled"))?;
    }
    if settings.fast_mode {
        apply("service_tier", value("fast"))?;
        apply("features.fast_mode", value(true))?;
    }
    let path = home.join(STATE_FILE);
    let old_state = read_state(home)?;
    let state_changed = !state.0.is_empty() || path.exists();
    if state_changed {
        write_bytes_atomic(
            &path,
            &serde_json::to_vec_pretty(&state).map_err(|e| e.to_string())?,
        )?;
    }
    if let Err(error) =
        write_bytes_atomic(&home.join("config.toml"), document.to_string().as_bytes())
    {
        if state_changed {
            let bytes = serde_json::to_vec(&old_state).map_err(|e| e.to_string())?;
            if let Err(rollback) = write_bytes_atomic(&path, &bytes) {
                return Err(format!("{error}；恢复 API 设置备份失败：{rollback}"));
            }
        }
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{account::AccountStore, api_model_catalog};

    fn enabled() -> ApiModelSettings {
        ApiModelSettings {
            auto_compact_enabled: true,
            reasoning_enabled: true,
            disable_web_search: true,
            fast_mode: true,
            reasoning_effort: "xhigh".into(),
            ..Default::default()
        }
    }

    fn install(home: &Path, account_id: &str, settings: &ApiModelSettings) -> Document {
        let models =
            crate::parse_codex_api_key_models(r#"{"data":[{"id":"gpt-5.6"},{"id":"fixture"}]}"#)
                .unwrap();
        let catalog = api_model_catalog::encode_with_settings(&models, settings).unwrap();
        api_model_catalog::install_with_settings(home, account_id, &catalog, settings).unwrap();
        std::fs::read_to_string(home.join("config.toml"))
            .unwrap()
            .parse()
            .unwrap()
    }

    #[test]
    fn defaults_and_validation_are_safe_for_existing_settings() {
        let defaults: ApiModelSettings = serde_json::from_str("{}").unwrap();
        assert!(
            !defaults.auto_compact_enabled
                && !defaults.reasoning_enabled
                && !defaults.disable_web_search
                && !defaults.fast_mode
        );
        assert_eq!(defaults.auto_compact_token_limit, 180_000);
        let mut settings = enabled();
        assert!(settings.validate().is_ok());
        settings.auto_compact_token_limit = 0;
        assert!(settings.validate().is_err());
        settings.auto_compact_token_limit = 180_000;
        settings.reasoning_levels.push("xhigh".into());
        assert!(settings.validate().is_err());
        settings.reasoning_levels = vec!["low".into()];
        assert!(settings.validate().is_err());
        settings.reasoning_levels = vec!["Bad Name".into()];
        settings.reasoning_effort = "Bad Name".into();
        assert!(settings.validate().is_err());
    }

    #[test]
    fn disabling_overrides_restores_original_values_and_preserves_other_settings() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join("config.toml"), "notify = ['fixture']\nmodel_reasoning_effort = 'medium'\nmodel_auto_compact_token_limit = 90000\nweb_search = 'cached'\nservice_tier = 'flex'\n[features]\nfast_mode = false\njs_repl = false\n").unwrap();
        let doc = install(home.path(), "a", &enabled());
        assert_eq!(
            doc["model_auto_compact_token_limit"].as_integer(),
            Some(180_000)
        );
        assert_eq!(doc["model_reasoning_effort"].as_str(), Some("xhigh"));
        assert_eq!(doc["web_search"].as_str(), Some("disabled"));
        assert_eq!(doc["service_tier"].as_str(), Some("fast"));
        assert_eq!(doc["features"]["fast_mode"].as_bool(), Some(true));
        // Changing the threshold must not replace the original backup with our previous value.
        let mut next = enabled();
        next.auto_compact_token_limit = 200_000;
        install(home.path(), "b", &next);
        let restored = install(home.path(), "b", &ApiModelSettings::default());
        assert_eq!(
            restored["model_auto_compact_token_limit"].as_integer(),
            Some(90_000)
        );
        assert_eq!(restored["model_reasoning_effort"].as_str(), Some("medium"));
        assert_eq!(restored["web_search"].as_str(), Some("cached"));
        assert_eq!(restored["service_tier"].as_str(), Some("flex"));
        assert_eq!(restored["features"]["fast_mode"].as_bool(), Some(false));
        assert_eq!(
            restored["notify"]
                .as_array()
                .unwrap()
                .get(0)
                .unwrap()
                .as_str(),
            Some("fixture")
        );
        assert_eq!(restored["features"]["js_repl"].as_bool(), Some(false));
    }

    #[test]
    fn manual_edits_win_and_absent_original_keys_are_removed() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join("config.toml"), "").unwrap();
        let mut doc = install(home.path(), "a", &enabled());
        doc["model_auto_compact_token_limit"] = value(123456);
        doc["service_tier"] = value("default");
        doc["features"]["fast_mode"] = value(false);
        std::fs::write(home.path().join("config.toml"), doc.to_string()).unwrap();
        let restored = install(home.path(), "a", &ApiModelSettings::default());
        assert_eq!(
            restored["model_auto_compact_token_limit"].as_integer(),
            Some(123456)
        );
        assert!(restored.get("model_reasoning_effort").is_none());
        assert!(restored.get("web_search").is_none());
        assert_eq!(restored["service_tier"].as_str(), Some("default"));
        assert_eq!(restored["features"]["fast_mode"].as_bool(), Some(false));
    }

    #[test]
    fn fast_mode_preserves_feature_tables_and_rejects_invalid_tables_without_writing() {
        for original in [
            "",
            "features = {js_repl = false}",
            "[features.code_mode]\nenabled = true\n",
        ] {
            let home = tempfile::tempdir().unwrap();
            std::fs::write(home.path().join("config.toml"), original).unwrap();
            let settings = ApiModelSettings {
                fast_mode: true,
                ..Default::default()
            };
            install(home.path(), "fixture", &settings);
            let doc = install(home.path(), "fixture", &ApiModelSettings::default());
            assert!(doc.get("service_tier").is_none());
            assert!(doc
                .get("features")
                .and_then(|f| f.get("fast_mode"))
                .is_none());
            if original.contains("js_repl") {
                assert_eq!(doc["features"]["js_repl"].as_bool(), Some(false));
            }
            if original.contains("code_mode") {
                assert_eq!(
                    doc["features"]["code_mode"]["enabled"].as_bool(),
                    Some(true)
                );
            }
        }
        let home = tempfile::tempdir().unwrap();
        let original = "features = 'invalid'\n";
        std::fs::write(home.path().join("config.toml"), original).unwrap();
        let mut document = original.parse::<Document>().unwrap();
        let settings = ApiModelSettings {
            fast_mode: true,
            ..Default::default()
        };
        assert!(write_config(home.path(), &mut document, &settings, None).is_err());
        assert_eq!(
            std::fs::read_to_string(home.path().join("config.toml")).unwrap(),
            original
        );
        assert!(!home.path().join(STATE_FILE).exists());
    }

    #[test]
    fn api_to_api_to_official_does_not_pollute_managed_model_backup() {
        let home = tempfile::tempdir().unwrap();
        let storage = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("config.toml"),
            "model = 'original'\nmodel_reasoning_effort = 'medium'\nweb_search = 'live'\n",
        )
        .unwrap();
        let store = AccountStore::new(storage.path().to_path_buf(), home.path().to_path_buf());
        let a = store
            .add_api_key_account(
                "fixture-a".into(),
                Some("https://a.example/v1".into()),
                Some("A".into()),
                None,
                None,
            )
            .unwrap();
        let b = store
            .add_api_key_account(
                "fixture-b".into(),
                Some("https://b.example/v1".into()),
                Some("B".into()),
                None,
                None,
            )
            .unwrap();
        let oauth = store.import_from_json(&serde_json::json!({"email":"fixture@example.com","tokens":{"access_token":"fixture","id_token":"fixture","refresh_token":"fixture"}}).to_string()).unwrap().remove(0);
        // Give B a managed default model before starting the tested switch chain.
        store.switch_account(&b.id).unwrap();
        store
            .update_api_key_default_model(&b.id, "gpt-5.6".into())
            .unwrap();
        store.switch_account(&oauth.id).unwrap();
        store.switch_account(&a.id).unwrap();
        install(home.path(), &a.id, &enabled());
        store.switch_account(&b.id).unwrap();
        install(home.path(), &b.id, &enabled());
        store.switch_account(&oauth.id).unwrap();
        let doc: Document = std::fs::read_to_string(home.path().join("config.toml"))
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(doc["model_reasoning_effort"].as_str(), Some("medium"));
        assert_eq!(doc["web_search"].as_str(), Some("live"));
        assert!(doc.get("model_auto_compact_token_limit").is_none());
        assert!(doc.get("model_catalog_json").is_none());
        assert!(doc.get("service_tier").is_none());
        assert!(doc
            .get("features")
            .and_then(|f| f.get("fast_mode"))
            .is_none());
        assert!(read_state(home.path()).unwrap().0.is_empty());
    }
}
