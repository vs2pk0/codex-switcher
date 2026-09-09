use std::{fs, path::Path};

/// Invalid catalogs are detached, never replaced with invented models. Keep
/// both the catalog and a TOML backup for diagnosis and manual recovery.
pub(crate) fn repair_invalid_catalog_references(home: &Path) -> Result<bool, String> {
    let path = home.join("config.toml");
    if !path.exists() {
        return Ok(false);
    }
    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut doc = text
        .parse::<toml_edit::Document>()
        .map_err(|e| e.to_string())?;
    let mut changed = detach_invalid_catalog(doc.as_table_mut(), home);
    if let Some(profiles) = doc
        .get_mut("profiles")
        .and_then(toml_edit::Item::as_table_like_mut)
    {
        for (_, profile) in profiles.iter_mut() {
            if let Some(table) = profile.as_table_like_mut() {
                changed |= detach_invalid_catalog(table, home);
            }
        }
    }
    if changed {
        fs::copy(
            &path,
            home.join(format!(
                "config.toml.catalog-repair-{:016x}.bak",
                rand::random::<u64>()
            )),
        )
        .map_err(|e| format!("备份模型目录配置失败：{e}"))?;
        crate::account::write_bytes_atomic(&path, doc.to_string().as_bytes())?;
    }
    Ok(changed)
}

fn detach_invalid_catalog(table: &mut dyn toml_edit::TableLike, home: &Path) -> bool {
    let Some(item) = table.get("model_catalog_json") else {
        return false;
    };
    let valid = item.as_str().is_some_and(|value| {
        let path = if let Some(rest) = value.strip_prefix("~/") {
            dirs::home_dir().unwrap_or_default().join(rest)
        } else {
            home.join(value)
        };
        if !fs::metadata(&path)
            .is_ok_and(|metadata| metadata.is_file() && metadata.len() <= 32 * 1024 * 1024)
        {
            return false;
        }
        fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|value| {
                value
                    .get("models")
                    .and_then(|models| models.as_array())
                    .cloned()
            })
            .is_some_and(|models| {
                !models.is_empty()
                    && models.iter().all(|model| {
                        model
                            .get("slug")
                            .and_then(|slug| slug.as_str())
                            .is_some_and(|slug| !slug.trim().is_empty())
                    })
            })
    });
    if !valid {
        table.remove("model_catalog_json");
    }
    !valid
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detaches_empty_missing_and_invalid_catalogs_but_keeps_valid_profiles() {
        let home = tempfile::tempdir().unwrap();
        fs::write(home.path().join("empty.json"), r#"{"models":[]}"#).unwrap();
        fs::write(
            home.path().join("valid.json"),
            r#"{"models":[{"slug":"test/model"}]}"#,
        )
        .unwrap();
        fs::write(home.path().join("config.toml"), "model_catalog_json = 'empty.json'\n[profiles.good]\nmodel_catalog_json = 'valid.json'\n[profiles.bad]\nmodel_catalog_json = 'missing.json'\n").unwrap();
        assert!(repair_invalid_catalog_references(home.path()).unwrap());
        let text = fs::read_to_string(home.path().join("config.toml")).unwrap();
        assert!(!text.contains("empty.json"));
        assert!(!text.contains("missing.json"));
        assert!(text.contains("valid.json"));
        assert!(!repair_invalid_catalog_references(home.path()).unwrap());
        assert!(home.path().join("empty.json").exists());
    }
}
