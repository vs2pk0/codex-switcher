//! Instance-local model discovery for direct API Key accounts.
use crate::{account::write_bytes_atomic, CodexApiKeyModel};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;
use toml_edit::{value, Document};

fn filename(account_id: &str) -> String {
    format!(
        "switcher-api-models-{:x}.json",
        Sha256::digest(account_id.as_bytes())
    )
}

/// A plain /models response often has IDs only. Absence is not an explicit
/// text-only capability. Accept only Codex's closed modality enum.
pub(crate) fn parse_input_modalities(model: &Value) -> Option<Vec<String>> {
    let declared = model
        .get("input_modalities")
        .or_else(|| model.get("inputModalities"))
        .or_else(|| model.pointer("/architecture/input_modalities"));
    if let Some(values) = declared.and_then(Value::as_array) {
        let accepted: Vec<String> = ["text", "image", "audio"]
            .into_iter()
            .filter(|kind| values.iter().any(|v| v.as_str() == Some(*kind)))
            .map(str::to_owned)
            .collect();
        if !accepted.is_empty() {
            return Some(accepted);
        }
    }
    model
        .get("supports_vision")
        .and_then(Value::as_bool)
        .map(|vision| {
            if vision {
                vec!["text".into(), "image".into()]
            } else {
                vec!["text".into()]
            }
        })
}

/// Drop only our own catalog reference when changing account/provider.
pub(crate) fn detach_other_catalog(document: &mut Document, home: &Path, account_id: Option<&str>) {
    let Some(path) = document
        .get("model_catalog_json")
        .and_then(|item| item.as_str())
    else {
        return;
    };
    let path = Path::new(path);
    let owned = path.parent() == Some(home)
        && path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|name| {
                name.strip_prefix("switcher-api-models-")
                    .and_then(|s| s.strip_suffix(".json"))
                    .is_some_and(|hash| {
                        hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())
                    })
            });
    if owned && !account_id.is_some_and(|id| path == home.join(filename(id))) {
        document.remove("model_catalog_json");
    }
}

fn catalog(models: &[CodexApiKeyModel]) -> Result<Value, String> {
    if models.is_empty() {
        return Err("API 未返回模型，未切换账号；请检查模型接口后重试".into());
    }
    Ok(
        json!({"models": models.iter().enumerate().map(|(index, model)| json!({
        "slug": model.id, "display_name": model.id,
        "description": "Model advertised by the configured API provider",
        "visibility": "list", "supported_in_api": true, "priority": index,
        "shell_type": "unified_exec", "base_instructions": "You are a helpful coding assistant.",
        "default_reasoning_level": null, "supported_reasoning_levels": [],
        "supports_reasoning_summaries": false, "support_verbosity": false,
        "supports_parallel_tool_calls": false, "prefer_websockets": false,
        "truncation_policy": {"mode": "tokens", "limit": 10000},
        // Keep uploads available for ID-only listings; the upstream API remains
        // authoritative. Explicit text-only metadata still disables images.
        "input_modalities": model.input_modalities.clone().unwrap_or_else(|| vec!["text".into(), "image".into()]),
        "experimental_supported_tools": []
    })).collect::<Vec<_>>()}),
    )
}

pub(crate) fn encode(models: &[CodexApiKeyModel]) -> Result<Vec<u8>, String> {
    serde_json::to_vec_pretty(&catalog(models)?).map_err(|e| e.to_string())
}

pub(crate) fn install(home: &Path, account_id: &str, contents: &[u8]) -> Result<(), String> {
    let config = home.join("config.toml");
    let mut document = std::fs::read_to_string(&config)
        .map_err(|e| e.to_string())?
        .parse::<Document>()
        .map_err(|e| e.to_string())?;
    let path = home.join(filename(account_id));
    write_bytes_atomic(&path, contents)?;
    document["model_catalog_json"] = value(path.to_string_lossy().into_owned());
    write_bytes_atomic(&config, document.to_string().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn publishes_all_models_and_detaches_only_owned_catalogs() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join("config.toml"), "model_provider = 'test'\n").unwrap();
        let models = (0..25)
            .map(|n| CodexApiKeyModel {
                id: format!("model-{n}"),
                owned_by: None,
                input_modalities: None,
            })
            .collect::<Vec<_>>();
        let encoded = encode(&models).unwrap();
        install(home.path(), "account-a", &encoded).unwrap();
        let mut doc = std::fs::read_to_string(home.path().join("config.toml"))
            .unwrap()
            .parse::<Document>()
            .unwrap();
        let data: Value = serde_json::from_slice(
            &std::fs::read(doc["model_catalog_json"].as_str().unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(data["models"].as_array().unwrap().len(), 25);
        detach_other_catalog(&mut doc, home.path(), Some("account-a"));
        assert!(doc.get("model_catalog_json").is_some());
        detach_other_catalog(&mut doc, home.path(), Some("account-b"));
        assert!(doc.get("model_catalog_json").is_none());
        doc["model_catalog_json"] = value("custom-catalog.json");
        detach_other_catalog(&mut doc, home.path(), None);
        assert_eq!(
            doc["model_catalog_json"].as_str(),
            Some("custom-catalog.json")
        );
        assert!(encode(&[]).is_err());
    }

    #[test]
    #[ignore = "requires CODEX_MODEL_CATALOG_TEST_BINARY and Node.js; uses temporary data"]
    fn real_codex_lists_all_advertised_models() {
        let binary = std::env::var("CODEX_MODEL_CATALOG_TEST_BINARY").unwrap();
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join("config.toml"),
            "model_provider = 'fixture'\n[model_providers.fixture]\nname = 'fixture'\nbase_url = 'http://127.0.0.1:1/v1'\nwire_api = 'responses'\nrequires_openai_auth = false\nexperimental_bearer_token = 'fixture'\n").unwrap();
        let models = (0..25)
            .map(|n| CodexApiKeyModel {
                id: format!("fixture-{n}"),
                owned_by: None,
                input_modalities: match n {
                    0 => Some(vec!["text".into(), "image".into()]),
                    1 => Some(vec!["text".into()]),
                    _ => None,
                },
            })
            .collect::<Vec<_>>();
        install(home.path(), "fixture", &encode(&models).unwrap()).unwrap();
        let output = std::process::Command::new("node").args(["--input-type=module", "-e", r#"
            import {spawn} from 'node:child_process';
            import {createInterface} from 'node:readline';
            const child = spawn(process.argv[1], ['app-server'], {
              cwd: process.argv[2], env: {...process.env, CODEX_HOME: process.argv[2]}, stdio: ['pipe','pipe','pipe']
            });
            const timer = setTimeout(() => { child.kill(); process.exitCode = 1; }, 15000);
            const send = value => child.stdin.write(JSON.stringify(value)+'\n');
            child.stderr.on('data', data => process.stderr.write(data));
            let passed = false;
            createInterface({input: child.stdout}).on('line', line => {
              const message = JSON.parse(line);
              if(message.id === 1) {
                send({method:'initialized'});
                send({id:2,method:'model/list',params:{includeHidden:false}});
              }
              if(message.id === 2) {
                const ids = (message.result?.data ?? []).map(row => row.model);
                passed = Array.from({length:25},(_,n)=>'fixture-'+n).every(id => ids.includes(id));
                const rows = message.result?.data ?? [];
                const modalities = id => rows.find(row => row.model === id)?.inputModalities ?? [];
                passed &&= modalities('fixture-0').includes('image')
                  && !modalities('fixture-1').includes('image')
                  && modalities('fixture-2').includes('image');
                console.log(JSON.stringify(message));
                child.kill();
              }
            });
            child.on('exit', () => {clearTimeout(timer); process.exitCode = passed ? 0 : 1;});
            send({id:1,method:'initialize',params:{clientInfo:{name:'catalog-test',version:'1.0.0'}}});
        "#, &binary, home.path().to_str().unwrap()]).output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn model_list_capabilities_reach_catalog_without_disabling_unknown_vision() {
        let models = crate::parse_codex_api_key_models(
            r#"{"data":[
            {"id":"vision", "input_modalities":["text","image","video"]},
            {"id":"text", "input_modalities":["text"]},
            {"id":"architecture", "architecture":{"input_modalities":["text","image"]}},
            {"id":"unknown"},
            {"id":"explicit-no", "supports_vision":false},
            {"id":"malformed", "input_modalities":["video",null,5]}
        ]}"#,
        )
        .unwrap();
        let data = catalog(&models).unwrap();
        let rows = data["models"].as_array().unwrap();
        let modalities = |id: &str| {
            rows.iter().find(|row| row["slug"] == id).unwrap()["input_modalities"].clone()
        };
        for id in ["vision", "architecture", "unknown", "malformed"] {
            assert_eq!(modalities(id), json!(["text", "image"]));
        }
        for id in ["text", "explicit-no"] {
            assert_eq!(modalities(id), json!(["text"]));
        }
    }
}
