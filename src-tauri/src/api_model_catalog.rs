//! Instance-local model discovery for direct API Key accounts.
use crate::api_model_settings::{self, ApiModelSettings};
use crate::{account::write_bytes_atomic, CodexApiKeyModel};
use serde::Serialize;
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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ApiModelReasoning {
    /// `(effort, description)` in the provider's order (lowest to deepest).
    pub(crate) levels: Vec<(String, String)>,
    pub(crate) default: Option<String>,
}

/// Reasoning levels only when the provider declares them in Codex's own
/// catalog shape; a plain /models listing keeps Codex's no-slider default.
pub(crate) fn parse_reasoning(model: &Value) -> Option<ApiModelReasoning> {
    let mut levels: Vec<(String, String)> = vec![];
    for level in model
        .get("supported_reasoning_levels")?
        .as_array()?
        .iter()
        .take(16)
    {
        let (effort, description) = match level {
            Value::String(effort) => (effort.as_str(), ""),
            _ => (
                level.get("effort").and_then(Value::as_str).unwrap_or(""),
                level
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
            ),
        };
        if api_model_settings::valid_effort(effort) && !levels.iter().any(|(e, _)| e == effort) {
            let description: String = description
                .chars()
                .filter(|c| !c.is_control())
                .take(120)
                .collect();
            levels.push((effort.into(), description));
        }
    }
    let default = model
        .get("default_reasoning_level")
        .and_then(Value::as_str)
        .filter(|d| levels.iter().any(|(e, _)| e == d))
        .map(str::to_owned);
    Some(ApiModelReasoning { levels, default })
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

#[cfg(test)]
fn catalog(models: &[CodexApiKeyModel]) -> Result<Value, String> {
    catalog_with_settings(models, &ApiModelSettings::default())
}

fn catalog_with_settings(
    models: &[CodexApiKeyModel],
    settings: &ApiModelSettings,
) -> Result<Value, String> {
    settings.validate()?;
    if models.is_empty() {
        return Err("API 未返回模型，未切换账号；请检查模型接口后重试".into());
    }
    Ok(
        json!({"models": models.iter().enumerate().map(|(index, model)| {
        let mut reasoning = model.reasoning.clone().or_else(|| settings.reasoning_enabled.then(|| ApiModelReasoning {
            levels: settings.reasoning_levels.iter().map(|level| (level.clone(), String::new())).collect(),
            default: Some(settings.reasoning_effort.clone()),
        }));
        if settings.reasoning_enabled {
            if let Some(reasoning) = reasoning.as_mut() {
                reasoning.default = if reasoning.levels.iter().any(|(effort, _)| effort == &settings.reasoning_effort) {
                    Some(settings.reasoning_effort.clone())
                } else { reasoning.default.clone().or_else(|| reasoning.levels.first().map(|(effort, _)| effort.clone())) };
            }
        }
        json!({
        "slug": model.id, "display_name": model.id,
        "description": "Model advertised by the configured API provider",
        "visibility": "list", "supported_in_api": true, "priority": index,
        "shell_type": "unified_exec", "base_instructions": "You are a helpful coding assistant.",
        "default_reasoning_level": reasoning.as_ref().and_then(|r| r.default.clone()),
        "supported_reasoning_levels": reasoning.as_ref().map_or(vec![], |r| r.levels.iter()
            .map(|(effort, description)| json!({"effort": effort, "description": description}))
            .collect::<Vec<_>>()),
        "supports_reasoning_summaries": false, "support_verbosity": false,
        "supports_parallel_tool_calls": false, "prefer_websockets": false,
        // Codex normalizes the config alias `fast` to the catalog/wire ID `priority`.
        "service_tiers": if settings.fast_mode { json!([{"id":"priority", "name":"Fast", "description":"Fast processing, when supported by the API provider"}]) } else { json!([]) },
        "additional_speed_tiers": if settings.fast_mode { vec!["fast"] } else { vec![] },
        "default_service_tier": if settings.fast_mode { Some("priority") } else { None },
        "truncation_policy": {"mode": "tokens", "limit": 10000},
        // Keep uploads available for ID-only listings; the upstream API remains
        // authoritative. Explicit text-only metadata still disables images.
        "input_modalities": model.input_modalities.clone().unwrap_or_else(|| vec!["text".into(), "image".into()]),
        "experimental_supported_tools": []
    })}).collect::<Vec<_>>()}),
    )
}

#[cfg(test)]
pub(crate) fn encode(models: &[CodexApiKeyModel]) -> Result<Vec<u8>, String> {
    serde_json::to_vec_pretty(&catalog(models)?).map_err(|e| e.to_string())
}

pub(crate) fn encode_with_settings(
    models: &[CodexApiKeyModel],
    settings: &ApiModelSettings,
) -> Result<Vec<u8>, String> {
    serde_json::to_vec_pretty(&catalog_with_settings(models, settings)?).map_err(|e| e.to_string())
}

#[cfg(test)]
pub(crate) fn install(home: &Path, account_id: &str, contents: &[u8]) -> Result<(), String> {
    install_with_settings(home, account_id, contents, &ApiModelSettings::default())
}

pub(crate) fn install_with_settings(
    home: &Path,
    account_id: &str,
    contents: &[u8],
    settings: &ApiModelSettings,
) -> Result<(), String> {
    let config = home.join("config.toml");
    let mut document = std::fs::read_to_string(&config)
        .map_err(|e| e.to_string())?
        .parse::<Document>()
        .map_err(|e| e.to_string())?;
    api_model_settings::restore_document(home, &mut document)?;
    let catalog: Value = serde_json::from_slice(contents).map_err(|e| e.to_string())?;
    let selected_model = document.get("model").and_then(|item| item.as_str());
    let selected = catalog["models"].as_array().and_then(|models| {
        models
            .iter()
            .find(|row| row["slug"].as_str() == selected_model)
    });
    let effort = match selected {
        Some(model) => model["default_reasoning_level"].as_str(),
        None => Some(settings.reasoning_effort.as_str()),
    };
    let path = home.join(filename(account_id));
    write_bytes_atomic(&path, contents)?;
    document["model_catalog_json"] = value(path.to_string_lossy().into_owned());
    api_model_settings::write_config(home, &mut document, settings, effort)
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
                reasoning: None,
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
                reasoning: match n {
                    1 => Some(ApiModelReasoning {
                        levels: vec![
                            ("low".into(), String::new()),
                            ("high".into(), String::new()),
                        ],
                        default: Some("high".into()),
                    }),
                    3 => Some(ApiModelReasoning {
                        levels: vec![],
                        default: None,
                    }),
                    _ => None,
                },
            })
            .collect::<Vec<_>>();
        let settings = ApiModelSettings {
            reasoning_enabled: true,
            reasoning_effort: "xhigh".into(),
            auto_compact_enabled: true,
            disable_web_search: true,
            fast_mode: true,
            ..Default::default()
        };
        install_with_settings(
            home.path(),
            "fixture",
            &encode_with_settings(&models, &settings).unwrap(),
            &settings,
        )
        .unwrap();
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
                const row = id => rows.find(row => row.model === id);
                passed &&= row('fixture-0')?.defaultReasoningEffort === 'xhigh'
                  && JSON.stringify(row('fixture-0')?.supportedReasoningEfforts.map(v=>v.reasoningEffort)) === JSON.stringify(['low','medium','high','xhigh'])
                  && row('fixture-1')?.defaultReasoningEffort === 'high'
                  && row('fixture-1')?.supportedReasoningEfforts.length === 2
                  && row('fixture-3')?.supportedReasoningEfforts.length === 0
                  && row('fixture-0')?.serviceTiers.some(tier => tier.id === 'priority')
                  && row('fixture-0')?.defaultServiceTier === 'priority';
                if (!passed) console.error(JSON.stringify(message));
                send({id:3,method:'config/read',params:{includeLayers:false}});
              }
              if(message.id === 3) {
                const config = message.result?.config;
                passed &&= config?.web_search === 'disabled'
                  && config?.model_auto_compact_token_limit === 180000
                  && config?.model_reasoning_effort === 'xhigh'
                  && config?.service_tier === 'fast'
                  && config?.features?.fast_mode === true;
                if (!passed) console.error(JSON.stringify(message));
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
    #[ignore = "requires CODEX_MODEL_CATALOG_TEST_BINARY and Node.js; sends requests to a local fixture only"]
    fn real_codex_sends_fast_tier_only_when_enabled() {
        let binary = std::env::var("CODEX_MODEL_CATALOG_TEST_BINARY").unwrap();
        for fast_mode in [true, false] {
            let home = tempfile::tempdir().unwrap();
            std::fs::write(home.path().join("config.toml"),
            "model = 'fixture'\nmodel_provider = 'fixture'\n[model_providers.fixture]\nname = 'fixture'\nbase_url = 'http://127.0.0.1:1/v1'\nwire_api = 'responses'\nrequires_openai_auth = false\nexperimental_bearer_token = 'fixture'\n").unwrap();
            let models =
                crate::parse_codex_api_key_models(r#"{"data":[{"id":"fixture"}]}"#).unwrap();
            let settings = ApiModelSettings {
                fast_mode,
                disable_web_search: true,
                ..Default::default()
            };
            install_with_settings(
                home.path(),
                "fixture",
                &encode_with_settings(&models, &settings).unwrap(),
                &settings,
            )
            .unwrap();
            let output = std::process::Command::new("node").args(["--input-type=module", "-e", r#"
            import {spawn} from 'node:child_process';
            import {createInterface} from 'node:readline';
            import {createServer} from 'node:http';
            import {readFileSync,writeFileSync} from 'node:fs';
            const home = process.argv[2];
            const fastMode = process.argv[3] === 'true';
            let child;
            let passed = false;
            const server = createServer(async (request,response) => {
              let body = '';
              for await (const chunk of request) body += chunk;
              const value = JSON.parse(body);
              passed = value.model === 'fixture' && (fastMode
                ? value.service_tier === 'priority' : !Object.hasOwn(value, 'service_tier'));
              if (!passed) console.error('Unexpected model or service tier:', request.url, value.model, value.service_tier, Object.keys(value));
              response.writeHead(400, {'content-type':'application/json'});
              response.end(JSON.stringify({error:{message:'Intentional local test response',type:'invalid_request_error'}}));
              child.kill();
            });
            await new Promise(resolve => server.listen(0,'127.0.0.1',resolve));
            const configPath = home+'/config.toml';
            writeFileSync(configPath,readFileSync(configPath,'utf8').replace('127.0.0.1:1/', '127.0.0.1:'+server.address().port+'/'));
            child = spawn(process.argv[1],['app-server'],{
              cwd: home, env:{...process.env,CODEX_HOME:home}, stdio:['pipe','pipe','pipe']
            });
            const timer = setTimeout(()=>{console.error('No local API request received');child.kill();},20000);
            const send = value => child.stdin.write(JSON.stringify(value)+'\n');
            child.stderr.on('data', data => process.stderr.write(data));
            createInterface({input:child.stdout}).on('line',line=>{
              const message = JSON.parse(line);
              if(message.error) console.error(JSON.stringify(message));
              if(message.id === 1) {
                send({method:'initialized'});
                send({id:2,method:'thread/start',params:{model:'fixture',cwd:home,ephemeral:true,approvalPolicy:'never',sandbox:'read-only'}});
              }
              if(message.id === 2 && message.result?.thread?.id) {
                send({id:3,method:'turn/start',params:{threadId:message.result.thread.id,input:[{type:'text',text:'Reply OK',text_elements:[]}]}});
              }
            });
            child.on('exit',()=>{clearTimeout(timer);server.closeAllConnections();server.close();process.exitCode=passed?0:1;});
            send({id:1,method:'initialize',params:{clientInfo:{name:'fast-test',version:'1.0.0'}}});
        "#, &binary, home.path().to_str().unwrap(), if fast_mode { "true" } else { "false" }]).output().unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
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

    #[test]
    fn declared_reasoning_levels_reach_catalog_in_provider_order() {
        let models = crate::parse_codex_api_key_models(
            r#"{"data":[
            {"id":"glm", "default_reasoning_level":"max", "supported_reasoning_levels":[
                {"effort":"low","description":"轻量"},{"effort":"high"},"max",{"effort":"low"},
                {"effort":"Bad Name"},{"description":"no effort"},7]},
            {"id":"stale-default", "default_reasoning_level":"xhigh", "supported_reasoning_levels":["low","high"]},
            {"id":"plain"},
            {"id":"empty", "supported_reasoning_levels":[]}
        ]}"#,
        )
        .unwrap();
        let data = catalog(&models).unwrap();
        let row = |id: &str| {
            data["models"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["slug"] == id)
                .unwrap()
                .clone()
        };
        assert_eq!(
            row("glm")["supported_reasoning_levels"],
            json!([{"effort":"low","description":"轻量"},{"effort":"high","description":""},{"effort":"max","description":""}])
        );
        assert_eq!(row("glm")["default_reasoning_level"], "max");
        assert_eq!(row("stale-default")["default_reasoning_level"], Value::Null);
        for id in ["plain", "empty"] {
            assert_eq!(row(id)["supported_reasoning_levels"], json!([]));
            assert_eq!(row(id)["default_reasoning_level"], Value::Null);
        }
    }

    #[test]
    fn configured_reasoning_fills_missing_metadata_and_respects_declared_capabilities() {
        let models = crate::parse_codex_api_key_models(r#"{"data":[
            {"id":"plain"},
            {"id":"limited", "supported_reasoning_levels":["low","high"], "default_reasoning_level":"high"},
            {"id":"empty", "supported_reasoning_levels":[]}
        ]}"#).unwrap();
        let settings = ApiModelSettings {
            reasoning_enabled: true,
            reasoning_effort: "xhigh".into(),
            ..Default::default()
        };
        let contents = encode_with_settings(&models, &settings).unwrap();
        let data: Value = serde_json::from_slice(&contents).unwrap();
        let row = |id: &str| {
            data["models"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["slug"] == id)
                .unwrap()
        };
        assert_eq!(row("plain")["default_reasoning_level"], "xhigh");
        assert_eq!(
            row("plain")["supported_reasoning_levels"]
                .as_array()
                .unwrap()
                .len(),
            4
        );
        assert_eq!(row("limited")["default_reasoning_level"], "high");
        assert_eq!(
            row("limited")["supported_reasoning_levels"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(row("empty")["supported_reasoning_levels"], json!([]));
        assert_eq!(row("empty")["default_reasoning_level"], Value::Null);
        for (model, expected) in [
            ("plain", Some("xhigh")),
            ("limited", Some("high")),
            ("empty", None),
        ] {
            let home = tempfile::tempdir().unwrap();
            std::fs::write(
                home.path().join("config.toml"),
                format!("model = '{model}'\nmodel_reasoning_effort = 'medium'\n"),
            )
            .unwrap();
            install_with_settings(home.path(), "fixture", &contents, &settings).unwrap();
            let doc: Document = std::fs::read_to_string(home.path().join("config.toml"))
                .unwrap()
                .parse()
                .unwrap();
            assert_eq!(
                doc.get("model_reasoning_effort")
                    .and_then(|item| item.as_str()),
                expected
            );
            install_with_settings(
                home.path(),
                "fixture",
                &contents,
                &ApiModelSettings::default(),
            )
            .unwrap();
            let doc: Document = std::fs::read_to_string(home.path().join("config.toml"))
                .unwrap()
                .parse()
                .unwrap();
            assert_eq!(doc["model_reasoning_effort"].as_str(), Some("medium"));
        }
    }
}
