//! The file import graph and the file and directory cycle detectors.
use super::*;
use octocode_engine::graph::FileGraphNode;

/// The file graph the stored tables hold, over the files `keep` admits:
/// each production `imports` edge between them under its kind, and each
/// Rust `mod` declaration (a `contains` edge) as a module-tree edge — the
/// input of the one cycle engine ([`Cycles`]). Test edges close no cycle,
/// and neither do `use` edges within one Rust module tree (`use super::x`
/// from a child, `use child::y` from its parent): the `mod` edge already
/// draws that tree once, downward.
pub(crate) fn file_graph(
    t: &GraphTables,
    keep: &dyn Fn(u32) -> bool,
) -> BTreeMap<String, FileGraphNode> {
    let mut graph = BTreeMap::<String, FileGraphNode>::new();
    for (id, node) in t.nodes.iter().enumerate() {
        let Ok(id) = u32::try_from(id) else { break };
        if node.kind != NodeKind::File || !keep(id) {
            continue;
        }
        let entry = graph.entry(t.str(node.key).to_owned()).or_default();
        for edge in t.out(id) {
            let target = &t.nodes[edge.dst as usize];
            if target.kind != NodeKind::File || !keep(edge.dst) || edge.test {
                continue;
            }
            let kind = match edge.kind {
                EdgeKind::Imports if rust_lineage(t, id, edge.dst) => continue,
                EdgeKind::Imports => t.str(edge.detail),
                EdgeKind::Contains if t.str(edge.detail) == MODULE_DETAIL => RUST_MODULE,
                _ => continue,
            };
            entry
                .edges
                .entry(t.str(target.key).to_owned())
                .or_default()
                .insert(
                    kind.to_owned(),
                    if edge.line == NONE { 0 } else { edge.line },
                );
        }
    }
    graph
}

pub(super) fn detect_cycles(c: &mut Ctx) {
    let v = &c.v;
    // Go forbids import cycles between packages; file-level SCCs inside one
    // package are normal.
    let graph = file_graph(v.t, &|f| v.file_level(f));
    let cycles = Cycles::of(&graph);
    let ids = v
        .files
        .iter()
        .map(|id| (v.key(*id), *id))
        .collect::<BTreeMap<_, _>>();
    let id_of = |file: &str| ids.get(file).copied();
    // Witnesses and cuts follow the dependency edges the groups were found on.
    let adj = |id: u32| {
        cycles
            .uses
            .get(v.key(id))
            .into_iter()
            .flat_map(|node| node.edges.keys())
            .filter_map(|target| id_of(target))
            .collect::<Vec<_>>()
    };
    let mut found = Vec::new();
    for (_, files) in cycles.groups() {
        let members = files.iter().filter_map(|f| id_of(f)).collect::<Vec<_>>();
        if members.is_empty() {
            continue;
        }
        let set = members.iter().copied().collect::<BTreeSet<_>>();
        let names = files.iter().cloned().collect::<BTreeSet<_>>();
        let runtime = !cycles.runtime_within(&names).is_empty();
        let witness = witness_cycle(members[0], &set, &adj);
        let cuts = feedback_arcs(&members, &adj);
        let rust = members.iter().all(|m| v.language(*m) == "rust");
        let impact = members.iter().map(|m| v.impact(*m)).fold(0.0, f64::max);
        let member_keys = members.iter().map(|m| v.key(*m)).collect::<Vec<_>>();
        let hop = |a: u32, b: u32| {
            let mut hop = json!({"from": v.key(a), "to": v.key(b)});
            if let Some(line) = v.line(a, b) {
                hop["line"] = json!(line);
            }
            hop
        };
        let evidence = json!({
            "size": members.len(),
            "runtime": runtime,
            "members": member_keys,
            "witness": witness.windows(2).map(|w| hop(w[0], w[1])).collect::<Vec<_>>(),
            "suggestedCuts": cuts.iter().map(|(a, b)| hop(*a, *b)).collect::<Vec<_>>(),
            "cutCount": cuts.len(),
        });
        let (kind, mut confidence, mut controls) = if runtime {
            (
                "Runtime import cycle",
                0.85,
                vec!["module-load imports alone close it"],
            )
        } else {
            (
                "Import cycle",
                0.35,
                vec![
                    "no module-load loop: it closes only through type-only, deferred or compile-time imports",
                ],
            )
        };
        // The groups are `graph query cycles`' over production files: a loop
        // that only test or config files close is not a production finding.
        controls.push("test and config files left out");
        if rust {
            confidence = f64::min(confidence, 0.35);
            controls.push("rust modules within one crate may legally cycle");
        }
        let first = v.key(witness[0]);
        let second = witness.get(1).map_or(first, |id| v.key(*id));
        found.push((
            member_keys[0].to_owned(),
            member_keys.join(","),
            format!(
                "{kind} of {} files; {} import(s) break it",
                members.len(),
                cuts.len()
            ),
            0.7 * magnitude(members.len()),
            confidence,
            impact,
            evidence,
            controls,
            vec![format!(
                "octocode graph query path {} {}",
                quote(second),
                quote(first)
            )],
        ));
    }
    for (subject, all, title, severity, confidence, impact, evidence, controls, verify) in found {
        push(
            &mut c.findings,
            Draft {
                detector: "cycle",
                subject,
                id_parts: &[&all],
                title,
                severity,
                confidence,
                impact,
                evidence,
                controls,
                verify,
            },
        );
    }
}

pub(super) fn detect_dir_cycles(c: &mut Ctx) {
    let v = &c.v;
    let mut dirs = BTreeMap::<&str, u32>::new();
    let mut names = Vec::new();
    for file in &v.files {
        if v.production(*file) && v.language(*file) != "go" {
            let dir = dirname(v.key(*file));
            if !dirs.contains_key(dir) {
                dirs.insert(dir, names.len() as u32);
                names.push(dir);
            }
        }
    }
    let mut adj = vec![BTreeMap::<u32, (u32, u32)>::new(); names.len()];
    for file in &v.files {
        let Some(&from) = dirs.get(dirname(v.key(*file))) else {
            continue;
        };
        for target in &v.out[*file as usize] {
            if let Some(&to) = dirs.get(dirname(v.key(*target)))
                && from != to
                && !rust_lineage(v.t, *file, *target)
            {
                adj[from as usize].entry(to).or_insert((*file, *target));
            }
        }
    }
    let nodes = (0..names.len() as u32).collect::<Vec<_>>();
    let succ = |id: u32| adj[id as usize].keys().copied().collect::<Vec<_>>();
    let components = strongly_connected(&nodes, &succ, names.len());
    let mut found = Vec::new();
    for members in components {
        let set = members.iter().copied().collect::<BTreeSet<_>>();
        let witness = witness_cycle(members[0], &set, &succ);
        let rust = witness
            .iter()
            .filter_map(|d| adj[*d as usize].values().next())
            .all(|(f, _)| v.language(*f) == "rust");
        let hops = witness
            .windows(2)
            .map(|w| {
                let (f, t) = adj[w[0] as usize][&w[1]];
                json!({"from": names[w[0] as usize], "to": names[w[1] as usize], "via": {"from": v.key(f), "to": v.key(t)}})
            })
            .collect::<Vec<_>>();
        let keys = members
            .iter()
            .map(|m| names[*m as usize])
            .collect::<Vec<_>>();
        found.push((keys.clone(), hops, rust));
    }
    for (keys, hops, rust) in found {
        let mut controls = vec!["directory projection of runtime file imports"];
        if rust {
            controls.push("rust module directories within one crate may legally cycle");
        }
        push(
            &mut c.findings,
            Draft {
                detector: "dir-cycle",
                subject: keys[0].to_owned(),
                id_parts: &[&keys.join(",")],
                title: format!(
                    "Directories depend on each other in a cycle ({} dirs)",
                    keys.len()
                ),
                severity: 0.5 * magnitude(keys.len()),
                confidence: if rust { 0.3 } else { 0.7 },
                impact: 0.5,
                evidence: json!({"dirs": keys, "witness": hops}),
                controls,
                verify: Vec::new(),
            },
        );
    }
}
