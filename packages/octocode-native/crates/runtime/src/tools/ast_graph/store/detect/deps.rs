//! Declared-dependency, boundary and unresolved-import detectors.
use super::*;

pub(super) fn detect_dependencies(c: &mut Ctx) {
    let v = &c.v;
    let t = v.t;
    let mut undeclared = BTreeMap::<(String, u32), Vec<(u32, u32)>>::new();
    let mut dev_in_prod = BTreeMap::<(String, u32), Vec<(u32, u32)>>::new();
    let mut hoisted = BTreeMap::<(String, u32), Vec<(u32, u32)>>::new();
    for edge in &t.edges {
        if edge.kind != EdgeKind::Imports || t.nodes[edge.dst as usize].kind != NodeKind::Package {
            continue;
        }
        let component = v
            .component
            .get(&edge.src)
            .map_or(".", |(_, name, _)| *name)
            .to_owned();
        match t.str(edge.detail) {
            "external-hoisted" if v.production(edge.src) => {
                hoisted
                    .entry((component, edge.dst))
                    .or_default()
                    .push((edge.src, edge.line));
            }
            "external-undeclared" if v.production(edge.src) => {
                undeclared
                    .entry((component, edge.dst))
                    .or_default()
                    .push((edge.src, edge.line));
            }
            "external-dev" if v.production(edge.src) => {
                dev_in_prod
                    .entry((component, edge.dst))
                    .or_default()
                    .push((edge.src, edge.line));
            }
            _ => {}
        }
    }
    let sites = |files: &[(u32, u32)]| {
        files
            .iter()
            .map(|(f, l)| {
                if *l == NONE {
                    json!({"file": v.key(*f)})
                } else {
                    json!({"file": v.key(*f), "line": l})
                }
            })
            .collect::<Vec<_>>()
    };
    let mut pending = Vec::new();
    for ((component, package), files) in &undeclared {
        let name = t.str(t.nodes[*package as usize].name).to_owned();
        pending.push((
            "undeclared-dependency",
            format!("{component}::{name}"),
            format!("`{name}` is imported but not declared in the {component} manifest"),
            0.8,
            0.75,
            json!({"package": name, "component": component, "importers": files.len(), "sites": sites(files)}),
            vec!["builtins, workspace siblings and ancestor manifests checked"],
        ));
    }
    for ((component, package), files) in &dev_in_prod {
        let name = t.str(t.nodes[*package as usize].name).to_owned();
        pending.push((
            "dev-dependency-in-production",
            format!("{component}::{name}"),
            format!("Production code imports dev-only dependency `{name}`"),
            0.6,
            0.7,
            json!({"package": name, "component": component, "importers": files.len(), "sites": sites(files)}),
            vec!["test and config files excluded"],
        ));
    }
    for ((component, package), files) in &hoisted {
        let name = t.str(t.nodes[*package as usize].name).to_owned();
        pending.push((
            "undeclared-dependency",
            format!("{component}::{name}"),
            format!("Phantom dependency: `{name}` resolves only because another workspace package declares it"),
            0.6,
            0.55,
            json!({"package": name, "component": component, "declaredBySibling": true, "importers": files.len(), "sites": sites(files)}),
            vec!["hoisted by the package manager; breaks when published or installed alone"],
        ));
    }
    push_pending(&mut c.findings, pending);
}

pub(super) fn detect_boundaries(c: &mut Ctx) {
    let v = &c.v;
    let mut entry_counts = BTreeMap::<&str, usize>::new();
    for file in v.entries.keys() {
        if let Some((dir, _, _)) = v.component.get(file) {
            *entry_counts.entry(dir).or_default() += 1;
        }
    }
    let entries_of = |dir: &str| entry_counts.get(dir).copied().unwrap_or(0);
    let mut crossings = BTreeMap::<(&str, &str), Vec<(u32, u32)>>::new();
    for file in &v.files {
        let Some((from, _, from_meta)) = v.component.get(file) else {
            continue;
        };
        if !v.production(*file) {
            continue;
        }
        for target in &v.out[*file as usize] {
            let Some((to, _, to_meta)) = v.component.get(target) else {
                continue;
            };
            // Package manifests with exports define a public surface (npm);
            // Go/Cargo import whole packages or crates, never single files.
            let npm = from_meta.starts_with("npm") && to_meta.starts_with("npm");
            if npm && from != to && !v.is_entry(*target) && entries_of(to) > 0 {
                crossings
                    .entry((from, to))
                    .or_default()
                    .push((*file, *target));
            }
        }
    }
    let mut pending = Vec::new();
    for ((from, to), pairs) in crossings {
        let evidence = json!({
            "from": from, "to": to, "imports": pairs.len(),
            "sites": pairs.iter().map(|(a, b)| json!({"from": v.key(*a), "to": v.key(*b), "line": v.line(*a, *b)})).collect::<Vec<_>>(),
        });
        pending.push((format!("{from} -> {to}"), pairs.len(), evidence, pairs[0].1));
    }
    for (subject, count, evidence, target) in pending {
        push(
            &mut c.findings,
            Draft {
                detector: "boundary-violation",
                subject,
                id_parts: &[],
                title: format!("Deep import into another package's internals ({count} import(s))"),
                severity: 0.45 * magnitude(count),
                confidence: 0.6,
                impact: c.v.impact(target),
                evidence,
                controls: vec!["targets that are package entrypoints/exports are allowed"],
                verify: Vec::new(),
            },
        );
    }
}

pub(super) fn detect_unresolved(c: &mut Ctx) {
    let t = c.v.t;
    let mut by_file = BTreeMap::<&str, Vec<Value>>::new();
    for diag in &t.diagnostics {
        if t.str(diag.code) == DiagnosticCode::UnresolvedInternal.as_str() {
            let mut site = json!({"message": t.str(diag.message)});
            if diag.line != NONE {
                site["line"] = json!(diag.line);
            }
            let file = t.str(diag.file);
            // Fixtures and generated code carry deliberately broken imports.
            let production = t.by_key(file).is_none_or(|id| {
                t.nodes[id as usize].flags
                    & (ROLE_TEST | ROLE_CONFIG | ROLE_NOT_AUTHORED | ROLE_DECLARATION)
                    == 0
            });
            if production {
                by_file.entry(file).or_default().push(site);
            }
        }
    }
    let pending = by_file
        .into_iter()
        .map(|(file, sites)| (file.to_owned(), sites))
        .collect::<Vec<_>>();
    for (file, sites) in pending {
        push(
            &mut c.findings,
            Draft {
                detector: "unresolved-import",
                subject: file.clone(),
                id_parts: &[],
                title: format!("{} internal import(s) could not be resolved", sites.len()),
                severity: 0.5 * magnitude(sites.len()),
                confidence: 0.8,
                impact: 0.5,
                evidence: json!({"count": sites.len(), "sites": sites}),
                controls: vec![
                    "relative/crate-internal specifiers only; packages are never unresolved",
                ],
                verify: vec![format!("octocode graph query diagnostics {}", quote(&file))],
            },
        );
    }
}
