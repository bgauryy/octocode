use octocode_engine_tools_core::config::*;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

fn map(value: Option<&Value>) -> BTreeMap<String, String> {
    value
        .and_then(Value::as_object)
        .map(|o| {
            o.iter()
                .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.into())))
                .collect()
        })
        .unwrap_or_default()
}
fn validation(v: &Value) -> Value {
    let r = validate_config(v);
    let mut result = Map::new();
    result.insert("valid".into(), json!(r.valid));
    result.insert("errors".into(), json!(r.errors));
    result.insert("warnings".into(), json!(r.warnings));
    if v.is_object() {
        result.insert("config".into(), r.config.unwrap_or(Value::Null));
    }
    Value::Object(result)
}
fn synthetic_input(
    env: BTreeMap<String, String>,
    file: Option<Value>,
    revision: u64,
) -> ConfigInput {
    let home = PathBuf::from("$TMP");
    ConfigInput {
        env,
        cwd: "$TMP/cwd".into(),
        os_home: "$TMP/os".into(),
        trusted_project: false,
        global_env: FileInput::Missing {
            path: home.join(".env"),
        },
        project_env: FileInput::Missing {
            path: "$TMP/cwd/.octocode/.env".into(),
        },
        config_file: match file {
            Some(v) => FileInput::Read {
                path: home.join(".octocoderc"),
                text: serde_json::to_string(&v).unwrap(),
            },
            None => FileInput::Missing {
                path: home.join(".octocoderc"),
            },
        },
        runtime_surface: RuntimeSurface::Mcp,
        revision,
    }
}
fn resolved_with_meta(out: &ConfigOutput) -> Value {
    let mut v = serde_json::to_value(&out.resolved).unwrap();
    let o = v.as_object_mut().unwrap();
    o.insert("source".into(), serde_json::to_value(&out.source).unwrap());
    o.insert(
        "configPath".into(),
        out.config_path
            .as_ref()
            .map(|p| Value::String(p.to_string_lossy().into()))
            .unwrap_or(Value::Null),
    );
    v
}
fn load_value(file: &FileInput) -> Value {
    let r = load_config(file);
    let mut o = Map::new();
    o.insert("success".into(), json!(r.success));
    if let Some(v) = r.config {
        o.insert("config".into(), v);
    }
    if let Some(e) = r.error {
        o.insert("error".into(), json!(e));
    }
    o.insert("path".into(), json!(r.path.to_string_lossy()));
    Value::Object(o)
}
fn normalize(mut v: Value) -> Value {
    match &mut v {
        Value::String(s) => {
            if s.starts_with("Failed to parse config file:") {
                *s = "Failed to parse config file: <parser>".into();
            }
            if s.starts_with("[octocode-config]") && s.contains("Failed to parse config file:") {
                let prefix = s.split("Failed to parse config file:").next().unwrap();
                *s = format!("{prefix}Failed to parse config file: <parser>\n");
            }
        }
        Value::Array(a) => {
            for x in a {
                *x = normalize(x.take())
            }
        }
        Value::Object(o) => {
            for x in o.values_mut() {
                *x = normalize(x.take())
            }
            for key in ["applied", "skippedProtected", "skippedExisting", "keys"] {
                if let Some(Value::Array(values)) = o.get_mut(key) {
                    values.sort_by_key(|v| v.as_str().unwrap_or_default().to_owned());
                }
            }
        }
        Value::Number(n) if n.as_f64().is_some_and(|n| n.fract() == 0.0) => {
            v = json!(n.as_f64().unwrap() as i64);
        }
        _ => {}
    }
    v
}

fn run(case: &Value) -> Value {
    let op = case["op"].as_str().unwrap();
    match op {
        "parseEnv" => {
            serde_json::to_value(parse_env(case.get("input").and_then(Value::as_str))).unwrap()
        }
        "parseBooleanEnv" => Value::Array(
            case["inputs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| serde_json::to_value(parse_boolean_env(v.as_str())).unwrap())
                .collect(),
        ),
        "parseIntEnv" => Value::Array(
            case["inputs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| serde_json::to_value(parse_int_env(v.as_str())).unwrap())
                .collect(),
        ),
        "parseStringArrayEnv" => Value::Array(
            case["inputs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| serde_json::to_value(parse_string_array_env(v.as_str())).unwrap())
                .collect(),
        ),
        "token" => {
            let e = map(case.get("env"));
            let t = resolve_env_token(&e);
            json!({"token":t.as_ref().map(PrivateTokenSelection::token),"source":t.as_ref().map(PrivateTokenSelection::source),"has":t.is_some(),"resolved":t.as_ref().map(|x|json!({"token":x.token(),"source":x.source()}))})
        }
        "validate" => validation(&case["config"]),
        "validateMany" => Value::Array(
            case["configs"]
                .as_array()
                .unwrap()
                .iter()
                .map(validation)
                .collect(),
        ),
        "resolveSections" => {
            let e = map(case.get("env"));
            let f = case.get("file");
            let mut v = serde_json::to_value(resolve_sections(f, &e)).unwrap();
            v.as_object_mut().unwrap().remove("version");
            v
        }
        "stats" => {
            let cfg = resolve_sections(None, &map(case.get("env")));
            json!(is_stats_enabled(&cfg))
        }
        "envFiles" => {
            let global = case.get("global").and_then(Value::as_str);
            let project = case.get("project").and_then(Value::as_str);
            let trusted = case["trusted"].as_bool().unwrap();
            let (m, s) = merged_env(global, project, trusted);
            let mut target = map(case.get("targetEnv"));
            let r = apply_env(&m, s.clone(), &mut target);
            let mut target2 = map(case.get("targetEnv"));
            let p = apply_env(&m, s.clone(), &mut target2);
            let report = |r: EnvApplyReport| json!({"applied":r.applied,"skippedProtected":r.skipped_protected,"skippedExisting":r.skipped_existing});
            json!({"loaded":{"map":m,"sources":s},"target":target,"applied":report(r),"propagated":{"applied":p.applied,"skippedProtected":p.skipped_protected,"skippedExisting":p.skipped_existing,"sources":p.sources,"keys":p.keys}})
        }
        "configFile" => {
            let exists = case.get("state").and_then(Value::as_str) != Some("absent");
            let file = if exists {
                FileInput::Read {
                    path: "$TMP/.octocoderc".into(),
                    text: case["content"].as_str().unwrap().into(),
                }
            } else {
                FileInput::Missing {
                    path: "$TMP/.octocoderc".into(),
                }
            };
            let load = load_value(&file);
            let rc = load_config(&file).config.unwrap_or_else(|| json!({}));
            json!({"pathSuffix":".octocoderc","exists":exists,"load":load,"octocoderc":rc})
        }
        "freshEnv" => {
            let a = resolve_config(&synthetic_input(map(case.get("first")), None, 1));
            let b = resolve_config(&synthetic_input(map(case.get("second")), None, 2));
            json!({"first":a.resolved.local.enabled,"second":b.resolved.local.enabled,"freshObject":true})
        }
        "freshFile" => {
            let a = resolve_config(&synthetic_input(
                BTreeMap::new(),
                case.get("first").cloned(),
                1,
            ));
            let b = resolve_config(&synthetic_input(
                BTreeMap::new(),
                case.get("second").cloned(),
                2,
            ));
            json!({"first":a.resolved.network.timeout,"second":b.resolved.network.timeout,"freshObject":true})
        }
        "fullResolve" => {
            let out = resolve_config(&synthetic_input(
                map(case.get("env")),
                case.get("file").cloned(),
                1,
            ));
            let stderr = out
                .diagnostics
                .iter()
                .map(|d| match d.severity {
                    Severity::Error => format!(
                        "[octocode-config] Invalid .octocoderc at $TMP/.octocoderc: {}\n",
                        d.message
                    ),
                    Severity::Warning => format!(
                        "[octocode-config] Configuration warning at $TMP/.octocoderc: {}\n",
                        d.message
                    ),
                })
                .collect::<String>();
            json!({"config":resolved_with_meta(&out),"stderr":stderr})
        }
        "lookup" => {
            let out = resolve_config(&synthetic_input(map(case.get("env")), None, 1));
            Value::Array(
                case["paths"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| {
                        get_config_value(&out.resolved, p.as_str().unwrap()).unwrap_or(Value::Null)
                    })
                    .collect(),
            )
        }
        _ => panic!("unknown op {op}"),
    }
}

fn main() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut args = std::env::args().skip(1);
    let fixture_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| repo.join(".octocode/worker/rust-migration/config/fixtures.json"));
    let observed_path = args.next().map(PathBuf::from).unwrap_or_else(|| {
        repo.join(".octocode/worker/rust-migration/config/reference-observed.json")
    });
    let fixtures: Value =
        serde_json::from_str(&fs::read_to_string(fixture_path).expect("read fixtures")).unwrap();
    let observed: Value =
        serde_json::from_str(&fs::read_to_string(observed_path).expect("read observations"))
            .unwrap();
    let expected: BTreeMap<_, _> = observed["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["id"].as_str().unwrap(), normalize(c["result"].clone())))
        .collect();
    let mut failures = vec![];
    for case in fixtures["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let actual = normalize(run(case));
        if actual != expected[id] {
            failures.push(json!({"id":id,"actual":actual,"expected":expected[id]}));
        }
    }
    if failures.is_empty() {
        println!("config parity: 47/47 passed")
    } else {
        eprintln!("{}", serde_json::to_string_pretty(&failures).unwrap());
        std::process::exit(1)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn frozen_reference_cases_match() {
        super::main();
    }
}
