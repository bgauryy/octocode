//! Temporary measurement harness (removed after use): complete localSearch
//! page walks with frozen-scan reuse (noIgnore:true) vs per-page rescans
//! (noIgnore:false), alternating modes per repetition.
use octocode_native::contracts::tool_types::LocalSearchQuery;
use octocode_native::policy::path::{PathPolicy, PathPolicyConfig};
use octocode_native::security::ContentSecurity;
use octocode_native::tools::cancel::NeverCancel;
use octocode_native::tools::local_search::execute_local_search;
use std::time::Instant;

fn walk(root: &str, term: &str, no_ignore: bool, page_size: u32, policy: &PathPolicy, security: &ContentSecurity) -> (f64, Vec<String>, u32) {
    let started = Instant::now();
    let mut evidence = Vec::new();
    let mut snapshot: Option<String> = None;
    let mut page = 1u32;
    let mut pages = 0;
    loop {
        let mut q = serde_json::json!({"path": root, "searchText": term, "noIgnore": no_ignore, "pageSize": page_size, "page": page, "maxMatchesPerFile": 1000, "goal": "m", "reasoning": "m", "sort": "path"});
        if let Some(s) = &snapshot { q["snapshot"] = serde_json::json!(s); }
        let query: LocalSearchQuery = serde_json::from_value(q).expect("query");
        let result = execute_local_search(&query, policy, security, &NeverCancel, None).unwrap_or_else(|e| panic!("{}", e.message));
        pages += 1;
        for f in &result.files {
            for m in f.matches.iter().flatten() { evidence.push(format!("{}:{}", f.path, m.line)); }
        }
        let Some(p) = result.pagination.as_ref() else { break };
        if snapshot.is_none() { snapshot = result.source_snapshot.clone(); }
        if !p.has_more { break; }
        page += 1;
    }
    (started.elapsed().as_secs_f64() * 1000.0, evidence, pages)
}

#[test]
#[ignore = "measurement harness; run explicitly in release"]
fn measure_walk() {
    let env = |k: &str| std::env::var(k).ok();
    let root = &env("WALK_ROOT").expect("WALK_ROOT");
    let term = &env("WALK_TERM").expect("WALK_TERM");
    let reps: usize = env("WALK_REPS").and_then(|s| s.parse().ok()).unwrap_or(7);
    let page_size: u32 = env("WALK_PAGE").and_then(|s| s.parse().ok()).unwrap_or(20);
    let policy = PathPolicy::new(PathPolicyConfig { workspace_root: Some(root.into()), ..Default::default() }).expect("policy");
    let security = ContentSecurity::new();
    let mut times = [Vec::new(), Vec::new()];
    let mut reference: Option<Vec<String>> = None;
    for rep in 0..reps {
        for i in 0..2 {
            let mode = (rep + i) % 2; // alternate first mode
            let (ms, ev, pages) = walk(root, term, mode == 1, page_size, &policy, &security);
            match &reference {
                None => reference = Some(ev.clone()),
                Some(r) => assert_eq!(r, &ev, "evidence differs between modes/runs"),
            }
            times[mode].push(ms);
            eprintln!("rep={rep} reuse={} pages={pages} hits={} ms={ms:.1}", mode == 1, ev.len());
        }
    }
    for (mode, label) in [(0, "rescan(noIgnore:false)"), (1, "reuse(noIgnore:true)")] {
        let mut t = times[mode].clone();
        t.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = t[t.len() / 2];
        let p95 = t[((t.len() as f64 * 0.95).ceil() as usize).saturating_sub(1).min(t.len() - 1)];
        println!("{label}: median={median:.1}ms p95={p95:.1}ms min={:.1} max={:.1} n={}", t[0], t[t.len()-1], t.len());
    }
}

fn preview(root: &str, policy: &PathPolicy, security: &ContentSecurity) -> (f64, String) {
    use octocode_native::tools::ast_rewrite::{AstRewriteRuntimeOptions, RewriteRequest, execute_ast_rewrite_with_options};
    let started = Instant::now();
    let request: RewriteRequest = serde_json::from_value(serde_json::json!({
        "ruleKind": "pattern", "path": root, "pattern": "console.log($$$A)", "rewrite": "console.debug($$$A)",
        "langType": "typescript", "maxFiles": 5000, "goal": "m", "reasoning": "m"
    })).expect("rewrite request");
    let value = execute_ast_rewrite_with_options(request, policy, security, &NeverCancel, &AstRewriteRuntimeOptions::default());
    let status = value["status"].as_str().unwrap_or("?").to_owned() + &format!(":{}", value.to_string().len());
    (started.elapsed().as_secs_f64() * 1000.0, status)
}

#[test]
#[ignore = "measurement harness; run explicitly in release"]
fn measure_rewrite_previews() {
    let a = std::env::var("PREVIEW_A").expect("PREVIEW_A");
    let b = std::env::var("PREVIEW_B").expect("PREVIEW_B");
    let reps: usize = std::env::var("WALK_REPS").ok().and_then(|s| s.parse().ok()).unwrap_or(5);
    let common = std::path::Path::new(&a).ancestors().find(|p| std::path::Path::new(&b).starts_with(p)).expect("common root").to_path_buf();
    let policy = std::sync::Arc::new(PathPolicy::new(PathPolicyConfig { workspace_root: Some(common), ..Default::default() }).expect("policy"));
    let security = std::sync::Arc::new(ContentSecurity::new());
    let (mut seq, mut conc, mut single_a, mut single_b) = (vec![], vec![], vec![], vec![]);
    for rep in 0..reps {
        let (ta, sa) = preview(&a, &policy, &security);
        let (tb, sb) = preview(&b, &policy, &security);
        single_a.push(ta); single_b.push(tb); seq.push(ta + tb);
        let started = Instant::now();
        let handles = [a.clone(), b.clone()].map(|root| {
            let (policy, security) = (policy.clone(), security.clone());
            std::thread::spawn(move || preview(&root, &policy, &security))
        });
        let outs: Vec<_> = handles.into_iter().map(|h| h.join().expect("thread")).collect();
        conc.push(started.elapsed().as_secs_f64() * 1000.0);
        eprintln!("rep={rep} a={ta:.1}({sa}) b={tb:.1}({sb}) concurrent={:.1} [{} {}]", conc[rep], outs[0].1, outs[1].1);
    }
    let med = |mut v: Vec<f64>| { v.sort_by(|x, y| x.partial_cmp(y).unwrap()); v[v.len() / 2] };
    let (ma, mb) = (med(single_a), med(single_b));
    println!("single A median={ma:.1}ms B median={mb:.1}ms; sequential median={:.1}ms; concurrent(global lock) median={:.1}ms; no-lock lower bound=max(A,B)={:.1}ms", med(seq), med(conc), ma.max(mb));
}
