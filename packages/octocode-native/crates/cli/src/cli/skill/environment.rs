use super::super::ToolRuntime;
use octocode_native::config::ENV_TOKEN_VARS;
use serde_json::{Value, json};

pub(super) fn status(runtime: &ToolRuntime, name: &str, github_auth: bool) -> Value {
    let mut params = Vec::new();
    let set = |key: &str| {
        runtime
            .config()
            .env_value(key)
            .is_some_and(|value| !value.trim().is_empty())
    };
    let github_skill = matches!(
        name,
        "octocode-research" | "octocode-pr-review" | "octocode-rfc-generator" | "octocode-roast"
    );
    // Only pull-request input needs GitHub here; local planning stays ready without a token.
    let github_pr_input = name == "octocode-test-plan";
    let github_ready = github_auth || ENV_TOKEN_VARS.iter().any(|key| set(key));
    if github_skill || github_pr_input {
        let (required, description) = if github_skill {
            (
                "recommended",
                "GitHub token — code search, file reads, repo/PR discovery",
            )
        } else {
            (
                "optional",
                "GitHub token — pull-request input only; local planning needs none",
            )
        };
        for (index, key) in ENV_TOKEN_VARS.iter().enumerate() {
            params.push(json!({
                "key": key, "status": if set(key) { "set" } else { "missing" },
                "required": required, "group": "github-token", "groupSatisfied": github_ready,
                "description": if index == 0 { description } else { "GitHub token (alternate name)" },
                "link": "https://github.com/settings/tokens"
            }));
        }
    }
    let mut optional = Vec::new();
    if matches!(name, "octocode-research" | "octocode-rfc-generator") {
        optional.push((
            super::super::config::env_key("classification.api"),
            "Optional semantic classification and evidence routing",
        ));
    }
    if name == "octocode-brainstorming" {
        optional.extend([
            (
                "TAVILY_API_KEY",
                "Optional Tavily search; host web tools work without it",
            ),
            ("EXA_API_KEY", "Optional Exa web, news and paper search"),
            (
                "SERPER_API_KEY",
                "Optional Google web, news and Scholar search",
            ),
        ]);
    }
    if name == "octocode-scraping" {
        optional.push((
            "SCRAPING_ANT",
            "Optional ScrapingAnt provider; direct public fetching needs no key",
        ));
    }
    for (key, description) in optional {
        let mut row = json!({"key":key,"status":if set(key){"set"}else{"missing"},"required":"optional","description":description});
        if key == super::super::config::env_key("classification.api") {
            row["group"] = json!("classification-key");
            row["groupSatisfied"] = json!(set(key));
            row["link"] = json!("https://console.typesafe.ai/keys");
        }
        params.push(row);
    }
    let readiness = if params.is_empty() {
        "ok"
    } else if github_skill && !github_ready {
        "partial"
    } else {
        "ready"
    };
    let hint = match (github_ready, github_skill, github_pr_input) {
        (false, true, _) => {
            "GitHub research needs a token: run octocode auth login or configure a GitHub token."
        }
        (false, false, true) => {
            "Pull-request input needs a GitHub token: run octocode auth login or configure a GitHub token. Local planning needs none."
        }
        _ => "",
    };
    json!({"readiness":readiness,"params":params,"hint":hint})
}
