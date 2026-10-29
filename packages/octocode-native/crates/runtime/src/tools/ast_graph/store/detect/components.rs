//! Component detectors (coupling, misplaced files) and the health summary.
use super::*;

/// Component-level Martin metrics: `unstable-dependency`, `main-sequence`.
/// Unit → units it imports (efferent) and units importing it (afferent),
/// over production files.
pub(super) fn unit_dependencies(
    v: &View,
    unit: &dyn Fn(u32) -> String,
) -> (
    BTreeMap<String, BTreeSet<String>>,
    BTreeMap<String, BTreeSet<String>>,
) {
    let mut efferent = BTreeMap::<String, BTreeSet<String>>::new();
    let mut afferent = BTreeMap::<String, BTreeSet<String>>::new();
    for file in &v.files {
        if !v.production(*file) {
            continue;
        }
        let from = unit(*file);
        efferent.entry(from.clone()).or_default();
        for target in &v.out[*file as usize] {
            if !v.production(*target) {
                continue;
            }
            let to = unit(*target);
            if to != from {
                efferent.entry(from.clone()).or_default().insert(to.clone());
                afferent.entry(to).or_default().insert(from.clone());
            }
        }
    }
    (efferent, afferent)
}

/// Unit → `(abstract, total)` exported top-level declarations: abstractness
/// from exported symbol kinds.
pub(super) fn unit_exports(
    v: &View,
    unit: &dyn Fn(u32) -> String,
) -> BTreeMap<String, (usize, usize)> {
    let t = v.t;
    let mut exported = BTreeMap::<String, (usize, usize)>::new();
    for node in &t.nodes {
        if node.kind == NodeKind::Symbol
            && node.flags & FLAG_EXPORTED != 0
            && node.parent == NONE
            && v.production(node.file)
        {
            let entry = exported.entry(unit(node.file)).or_default();
            entry.1 += 1;
            if ABSTRACT_KINDS.contains(&t.str(node.detail)) {
                entry.0 += 1;
            }
        }
    }
    exported
}

pub(super) fn detect_components(c: &mut Ctx) {
    let v = &c.v;
    let use_components = v
        .component
        .values()
        .map(|(dir, _, _)| *dir)
        .collect::<BTreeSet<_>>()
        .len()
        >= 2;
    let unit = |file: u32| -> String {
        if use_components {
            v.component
                .get(&file)
                .map_or(".".into(), |(dir, _, _)| (*dir).to_owned())
        } else {
            dirname(v.key(file)).to_owned()
        }
    };
    let level = if use_components {
        "package"
    } else {
        "directory"
    };
    let (efferent, afferent) = unit_dependencies(v, &unit);
    let instability = |name: &str| {
        let ce = efferent.get(name).map_or(0, BTreeSet::len) as f64;
        let ca = afferent.get(name).map_or(0, BTreeSet::len) as f64;
        if ca + ce == 0.0 { 0.0 } else { ce / (ca + ce) }
    };
    let exported = unit_exports(v, &unit);
    let mut pending = Vec::new();
    for (name, deps) in &efferent {
        let ca = afferent.get(name).map_or(0, BTreeSet::len);
        let ce = deps.len();
        if ca + ce < 3 {
            continue;
        }
        let i = instability(name);
        let worse = deps
            .iter()
            .filter(|d| instability(d) > i + 1e-9)
            .cloned()
            .collect::<Vec<_>>();
        if ce > 0 && worse.len() * 10 >= ce * 3 && !worse.is_empty() && ca > 0 {
            pending.push((
                "unstable-dependency",
                name.clone(),
                format!("Stable {level} depends on {} less stable {level}(s)", worse.len()),
                0.4,
                0.6,
                json!({"level": level, "instability": (i * 1000.0).round() / 1000.0, "ca": ca, "ce": ce,
                       "lessStable": worse.iter().map(|d| json!({"unit": d, "instability": (instability(d) * 1000.0).round() / 1000.0})).collect::<Vec<_>>()}),
                vec!["Arcan DoUD ≥ 30% of dependencies", "runtime imports between authored files"],
            ));
        }
        let (abstract_count, total) = exported.get(name).copied().unwrap_or((0, 0));
        if total >= 5 && ca + ce >= 5 {
            let a = abstract_count as f64 / total as f64;
            let d = (a + i - 1.0).abs();
            if d > 0.7 {
                let zone = if a + i < 1.0 {
                    "zone of pain (concrete and stable)"
                } else {
                    "zone of uselessness (abstract and unstable)"
                };
                pending.push((
                    "main-sequence",
                    name.clone(),
                    format!("Far from the main sequence (D={d:.2}): {zone}"),
                    0.3,
                    0.5,
                    json!({"level": level, "abstractness": (a * 1000.0).round() / 1000.0, "instability": (i * 1000.0).round() / 1000.0, "distance": (d * 1000.0).round() / 1000.0, "exports": total}),
                    vec!["abstractness from exported interface/trait/type declarations"],
                ));
            }
        }
    }
    push_pending(&mut c.findings, pending);
}

pub(super) fn detect_misplaced(c: &mut Ctx) {
    let v = &c.v;
    let mut pending = Vec::new();
    for file in &v.files {
        let key = v.key(*file);
        let name = key.rsplit('/').next().unwrap_or(key);
        let stem = name.split('.').next().unwrap_or(name);
        if !v.file_level(*file)
            || v.is_entry(*file)
            || matches!(stem, "index" | "mod" | "__init__" | "lib" | "main")
            || is_barrel(v, *file)
        {
            continue;
        }
        let home = dirname(key);
        let package = v.component.get(file).map(|(dir, _, _)| *dir);
        let mut by_dir = BTreeMap::<&str, usize>::new();
        let mut neighbors = v.out[*file as usize].clone();
        neighbors.extend(&v.inc[*file as usize]);
        neighbors.sort_unstable();
        neighbors.dedup();
        // A file moves within its package: neighbours in another package
        // are a layering question, not a placement one.
        neighbors.retain(|other| v.component.get(other).map(|(dir, _, _)| *dir) == package);
        for other in &neighbors {
            *by_dir.entry(dirname(v.key(*other))).or_default() += 1;
        }
        let same = by_dir.get(home).copied().unwrap_or(0);
        let total = neighbors.len();
        if let Some((dir, count)) = by_dir
            .iter()
            .filter(|(dir, _)| **dir != home)
            .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
            && *count >= 3
            && *count * 10 >= total * 6
            && same < *count
            // A Rust module's place is its `mod` declaration: depending on
            // the modules above it is the tree's normal direction.
            && !(v.language(*file) == "rust" && ancestor_dir(dir, home))
        {
            pending.push((*file, (*dir).to_owned(), *count, same, total));
        }
    }
    for (file, dir, count, same, total) in pending {
        push(
            &mut c.findings,
            Draft {
                detector: "misplaced-file",
                subject: c.v.key(file).to_owned(),
                id_parts: &[],
                title: format!("Most of its dependencies live in {dir}"),
                severity: 0.25,
                confidence: 0.5,
                impact: c.v.impact(file),
                evidence: json!({"suggestedDir": dir, "edgesToSuggested": count, "edgesToOwnDir": same, "edges": total}),
                controls: vec![
                    "index/mod/__init__, entries and barrels excluded",
                    "neighbours in other packages ignored",
                    "rust modules are not moved up their own module tree",
                ],
                verify: Vec::new(),
            },
        );
    }
}

/// Whether `dir` is `of` or a directory above it (`.` is above every path).
fn ancestor_dir(dir: &str, of: &str) -> bool {
    dir == "."
        || dir == of
        || of
            .strip_prefix(dir)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Lakos cumulative component dependency, normalized against a balanced
/// binary tree of the same size (NCCD > 1: more coupled than a tree).
pub(super) fn health(v: &View) -> Value {
    // The condensation of the cycle engine over production files: its
    // groups are the `cycle` detector's, so `filesInCycles` counts them.
    let graph = file_graph(v.t, &|f| v.production(f));
    let n = graph.len();
    if n == 0 {
        return json!({});
    }
    let cycles = Cycles::of(&graph);
    let sizes = cycles
        .condensed
        .components
        .iter()
        .map(Vec::len)
        .collect::<Vec<_>>();
    let k = sizes.len();
    let dag = (0..k)
        .map(|id| &cycles.condensed.edges[&id])
        .collect::<Vec<_>>();
    // Iterators borrow each adjacency set; the DFS and reachability workspace
    // both stay linear in the component count.
    let mut order = Vec::with_capacity(k);
    let mut seen = vec![false; k];
    for start in 0..k {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![(start, dag[start].iter())];
        while let Some((id, next)) = stack.last_mut() {
            if let Some(&target) = next.next() {
                if !seen[target] {
                    seen[target] = true;
                    stack.push((target, dag[target].iter()));
                }
            } else {
                order.push(*id);
                stack.pop();
            }
        }
    }
    // Stream 64 destination components at a time instead of retaining the
    // quadratic transitive-closure matrix. SCC weights preserve exact CCD.
    let mut reach = vec![0u64; k];
    let mut ccd = 0u64;
    for base in (0..k).step_by(64) {
        let end = (base + 64).min(k);
        let weight = sizes[base] as u64;
        let equal_weights = sizes[base..end].iter().all(|&size| size as u64 == weight);
        for &id in &order {
            let mut bits = if (base..end).contains(&id) {
                1u64 << (id - base)
            } else {
                0
            };
            for &next in dag[id] {
                bits |= reach[next];
            }
            reach[id] = bits;
            let reached = if equal_weights {
                u64::from(bits.count_ones()) * weight
            } else {
                let mut reached = 0u64;
                while bits != 0 {
                    reached += sizes[base + bits.trailing_zeros() as usize] as u64;
                    bits &= bits - 1;
                }
                reached
            };
            ccd += reached * sizes[id] as u64;
        }
    }
    let nf = n as f64;
    let balanced = (nf + 1.0) * (nf + 1.0).log2() - nf;
    let tangled = cycles.groups().map(|(_, files)| files.len()).sum::<usize>();
    json!({
        "files": n,
        "ccd": ccd,
        "nccd": if balanced > 0.0 { ((ccd as f64 / balanced) * 1000.0).round() / 1000.0 } else { 0.0 },
        "averageReach": ((ccd as f64 / nf) * 10.0).round() / 10.0,
        "filesInCycles": tangled,
    })
}
