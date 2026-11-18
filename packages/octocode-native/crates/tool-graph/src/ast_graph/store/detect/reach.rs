//! Reachability from entrypoints and unused-export detection.
use super::*;

/// Production files no production entrypoint reaches: `(file, orphan)`
/// rows, files only tests or support code (an example or bench that is a
/// Cargo entry) reach, and the count of skipped Python library files.
pub(super) fn unreached_files(v: &View, entries: &[u32]) -> (Vec<(u32, bool)>, Vec<u32>, usize) {
    // Support entries seed the non-production walk below, with the tests.
    let production_entries = entries
        .iter()
        .copied()
        .filter(|entry| v.flags(*entry) & ROLE_NOT_PRODUCTION == 0)
        .collect::<Vec<_>>();
    let prod = v.reach(&production_entries, false);
    // Test seeds: test/config files plus test functions living inside
    // production files (Rust `mod tests`, pytest functions).
    let tests = v
        .files
        .iter()
        .copied()
        .filter(|f| v.flags(*f) & ROLE_NOT_PRODUCTION != 0)
        .chain(v.t.node_ids().filter(|id| {
            let node = &v.t.nodes[*id as usize];
            node.kind == NodeKind::Symbol && node.flags & FLAG_TEST != 0
        }))
        .collect::<Vec<_>>();
    let test_reach = v.reach(&tests, true);
    // Edges into a file or its symbols from any other file, in one pass.
    let mut external_in = vec![0usize; v.t.nodes.len()];
    for edge in &v.t.edges {
        let (src, dst) = (v.file_of[edge.src as usize], v.file_of[edge.dst as usize]);
        if dst != NONE && src != dst {
            external_in[dst as usize] += 1;
        }
    }
    let mut unreachable = Vec::new();
    let mut test_only = Vec::new();
    let mut skipped_library = 0usize;
    for file in &v.files {
        if !v.file_level(*file) || prod[*file as usize] {
            continue;
        }
        if v.meta(*file).contains(";library") {
            skipped_library += 1;
            continue;
        }
        let orphan = external_in[*file as usize] == 0 && v.out[*file as usize].is_empty();
        if test_reach[*file as usize] {
            test_only.push(*file);
        } else {
            unreachable.push((*file, orphan));
        }
    }
    (unreachable, test_only, skipped_library)
}

pub(super) fn detect_reachability(c: &mut Ctx) {
    c.judges_file_level(&["unreachable-file", "test-only"]);
    let v = &c.v;
    let entries = v.entries.keys().copied().collect::<Vec<_>>();
    if entries.is_empty() {
        note(
            &mut c.notes,
            "unreachable-file",
            "no entrypoints could be inferred",
        );
        note(
            &mut c.notes,
            "test-only",
            "no entrypoints could be inferred",
        );
        return;
    }
    let (unreachable, test_only, skipped_library) = unreached_files(v, &entries);
    if unreachable.is_empty() && test_only.is_empty() {
        // An empty answer says what was checked: the files walked from how
        // many entrypoints (every one was reached, or skipped as noted).
        let files = v.files.iter().filter(|f| v.file_level(**f)).count();
        note_evaluated(
            &mut c.notes,
            &["unreachable-file", "test-only"],
            json!({"files": files, "entrypoints": entries.len()}),
        );
    }
    // One inferred entrypoint is more likely an incomplete entry set (a
    // second binary, a worker, a plugin missed), so a file it does not reach
    // is less likely dead; the evidence states the count.
    let entry_quality = if entries.len() >= 2 { 0.7 } else { 0.5 };
    let rules = v.entries.values().copied().collect::<BTreeSet<_>>();
    if skipped_library > 0 {
        note(
            &mut c.notes,
            "unreachable-file",
            &format!(
                "{skipped_library} file(s) in Python libraries skipped: every module is importable by consumers"
            ),
        );
    }
    for (file, orphan) in unreachable {
        let key = v.key(file).to_owned();
        let templates = v.meta(file).contains(";templates");
        let mut controls = vec![
            "every edge kind keeps a file alive (optimistic liveness)",
            "tests, configs, declarations and generated/bundled/vendored files excluded",
        ];
        if templates {
            controls.push("MDX/Vue/Svelte/Astro templates may import this file (not parsed)");
        }
        push(
            &mut c.findings,
            Draft {
                detector: "unreachable-file",
                subject: key.clone(),
                id_parts: &[],
                title: if orphan {
                    "Orphan file: nothing imports it and it imports nothing".into()
                } else {
                    "Not reachable from any inferred entrypoint".into()
                },
                severity: 0.6,
                confidence: entry_quality
                    * if orphan { 1.0 } else { 0.85 }
                    * if templates { 0.5 } else { 1.0 },
                impact: v.impact(file),
                evidence: json!({"orphan": orphan, "entrypoints": entries.len(), "entryRules": rules}),
                controls,
            },
        );
    }
    let classes = if test_only.is_empty() {
        Vec::new()
    } else {
        non_production_reach(v)
    };
    for file in test_only {
        let key = v.key(file).to_owned();
        let reached_by = classes
            .iter()
            .filter(|(_, _, reach)| reach[file as usize])
            .collect::<Vec<_>>();
        push(
            &mut c.findings,
            Draft {
                detector: "test-only",
                subject: key.clone(),
                id_parts: &[],
                title: format!(
                    "Production file used only by {}",
                    reached_by
                        .iter()
                        .map(|(_, who, _)| *who)
                        .collect::<Vec<_>>()
                        .join(" and ")
                ),
                severity: 0.45,
                confidence: entry_quality,
                impact: v.impact(file),
                evidence: json!({
                    "reachedBy": reached_by.iter().map(|(class, ..)| *class).collect::<Vec<_>>(),
                }),
                controls: vec![
                    "tests, configs and support code (examples, benches) treated as a separate entry class",
                ],
            },
        );
    }
}

/// Per class of non-production code (tests, configs, support code), its
/// name, who it is in a title, and the nodes its own walk reaches: a
/// `test-only` file names the classes that reach it. Tests seed from test
/// files and test functions inside production files (Rust `mod tests`,
/// pytest functions).
fn non_production_reach(v: &View) -> Vec<(&'static str, &'static str, Vec<bool>)> {
    [
        ("test", "tests", ROLE_TEST),
        ("config", "config files", ROLE_CONFIG),
        ("support", "support code", ROLE_SUPPORT),
    ]
    .into_iter()
    .map(|(class, who, role)| {
        let mut seeds = v
            .files
            .iter()
            .copied()
            .filter(|f| v.flags(*f) & role != 0)
            .collect::<Vec<_>>();
        if role == ROLE_TEST {
            seeds.extend(v.t.node_ids().filter(|id| {
                let node = &v.t.nodes[*id as usize];
                node.kind == NodeKind::Symbol && node.flags & FLAG_TEST != 0
            }));
        }
        (class, who, v.reach(&seeds, true))
    })
    .collect()
}

/// The public API, per exported name: the files whose every export is
/// public (entries, and what they `export *` or re-export as a namespace,
/// transitively) and the declarations those files re-export by name.
pub(super) struct PublicApi {
    pub(super) files: BTreeSet<u32>,
    pub(super) names: BTreeSet<u32>,
}

pub(super) fn public_api(v: &View<'_>) -> PublicApi {
    let t = v.t;
    let mut files = v.entries.keys().copied().collect::<BTreeSet<_>>();
    let mut names = BTreeSet::new();
    let mut queue = files.iter().copied().collect::<VecDeque<_>>();
    while let Some(file) = queue.pop_front() {
        for edge in t.out(file) {
            let to_file = t.nodes[edge.dst as usize].kind == NodeKind::File;
            let whole = match edge.kind {
                EdgeKind::Imports => t.str(edge.detail).ends_with("star-reexport"),
                EdgeKind::Uses if t.str(edge.detail) == REEXPORTED_NAME => {
                    if !to_file {
                        names.insert(edge.dst);
                    }
                    to_file
                }
                _ => false,
            };
            if whole && files.insert(edge.dst) {
                queue.push_back(edge.dst);
            }
        }
    }
    PublicApi { files, names }
}

pub(super) fn detect_unused_exports(c: &mut Ctx) {
    let v = &c.v;
    let t = v.t;
    let public = public_api(v);
    // An unreachable file is one `unreachable-file` finding: its exports
    // are not listed again.
    let entries = v.entries.keys().copied().collect::<Vec<_>>();
    let unreachable = if entries.is_empty() {
        BTreeSet::new()
    } else {
        unreached_files(v, &entries)
            .0
            .into_iter()
            .map(|(file, _)| file)
            .collect()
    };
    let namespace_used = t
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Uses && t.str(e.detail) == "namespace")
        .map(|e| e.dst)
        .collect::<BTreeSet<_>>();
    let dynamic = t
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Imports && t.str(e.detail) == "dynamic-import")
        .map(|e| e.dst)
        .collect::<BTreeSet<_>>();
    // Per file and by whether only tests use the export from another file:
    // each export's name and the line it is declared on. A test-used export
    // is a hypothesis (un-exporting needs the test changed), so it is never
    // grouped with exports nothing else touches.
    let mut by_file = BTreeMap::<(u32, bool), Vec<(&str, u32, u32)>>::new();
    let mut local_only = BTreeMap::<(u32, bool), Vec<&str>>::new();
    // Files whose exports this detector cannot judge, by language.
    let mut unjudged = BTreeMap::<&str, BTreeSet<u32>>::new();
    // Every top-level export, by the first rule that settled it.
    let mut settled = BTreeMap::<&str, usize>::new();
    for (id, node) in t.node_ids().zip(&t.nodes) {
        if node.kind != NodeKind::Symbol || node.flags & FLAG_EXPORTED == 0 || node.parent != NONE {
            continue;
        }
        let file = node.file;
        // Judged languages are never package-scoped, so production is
        // file-level here; the language check below discloses the rest.
        let excluded = [
            (!v.production(file), "notProduction"),
            (v.meta(file).contains(";library"), "library"),
            (v.is_entry(file), "entry"),
            (
                public.files.contains(&file) || public.names.contains(&id),
                "reexportedByEntry",
            ),
            (namespace_used.contains(&file), "namespaceImported"),
            (unreachable.contains(&file), "unreachableFile"),
        ]
        .into_iter()
        .find_map(|(applies, rule)| applies.then_some(rule));
        if let Some(rule) = excluded {
            *settled.entry(rule).or_default() += 1;
            continue;
        }
        if !exports_judged(v.key(file), t.str(node.detail)) {
            if !NAMED_IMPORT_LANGUAGES.contains(&v.language(file)) {
                unjudged.entry(v.language(file)).or_default().insert(file);
            }
            *settled.entry("notJudged").or_default() += 1;
            continue;
        }
        let users = other_file_users(v, id, file);
        if users == Users::Production {
            *settled.entry("usedByAnotherFile").or_default() += 1;
            continue;
        }
        let name = t.str(node.name);
        let tested = users == Users::Tests;
        // Referenced inside its own file: the export is unnecessary, the
        // code is not dead (knip `ignoreExportsUsedInFile`).
        let local = node.flags & FLAG_LOCAL_USE != 0
            || t.incoming(id).any(|e| {
                e.kind != EdgeKind::Contains && v.file_of[e.src as usize] == file && e.src != id
            });
        if local {
            local_only.entry((file, tested)).or_default().push(name);
        } else {
            by_file
                .entry((file, tested))
                .or_default()
                .push((name, node.line, id));
        }
    }
    note_unjudged_exports(&mut c.notes, &unjudged);
    if local_only.is_empty() && by_file.is_empty() {
        note_settled_exports(&mut c.notes, v, &settled);
    }
    for ((file, tested), names) in local_only {
        let key = v.key(file).to_owned();
        push(
            &mut c.findings,
            Draft {
                detector: "export-only-local",
                subject: key,
                id_parts: &[&names.join(",")],
                title: format!(
                    "{} export(s) used only inside this file{}: could be un-exported",
                    names.len(),
                    if tested { " and by tests" } else { "" }
                ),
                severity: 0.15,
                confidence: 0.8,
                impact: v.impact(file),
                evidence: json!({"exports": names, "count": names.len()}),
                controls: vec!["referenced within the file; not dead code"],
            },
        );
        tier_tested(&mut c.findings, tested);
    }
    for ((file, tested), exports) in by_file {
        let names = exports.iter().map(|(name, ..)| *name).collect::<Vec<_>>();
        let (_, line, _) = exports[0];
        push_unused_export(
            &mut c.findings,
            v,
            file,
            &names,
            line,
            dynamic.contains(&file),
            tested,
        );
        tier_tested(&mut c.findings, tested);
    }
}

/// The `unused-export` finding for `names` of `file`, the first declared on
/// `line`; `lazy` when the file is also imported dynamically.
fn push_unused_export(
    out: &mut Vec<Finding>,
    v: &View,
    file: u32,
    names: &[&str],
    line: u32,
    lazy: bool,
    tested: bool,
) {
    push(
        out,
        Draft {
            detector: "unused-export",
            subject: v.key(file).to_owned(),
            id_parts: &[&names.join(",")],
            title: format!(
                "{} export(s) with no importer outside the file{}",
                names.len(),
                if tested { " except tests" } else { "" }
            ),
            severity: 0.4 * magnitude(names.len()),
            confidence: if lazy { 0.3 } else { 0.6 },
            impact: v.impact(file),
            evidence: json!({"exports": names, "exportLine": line, "count": names.len(), "dynamicallyImported": lazy}),
            controls: vec![
                "entry files and the names they re-export (all of an `export *`) treated as public API",
                "namespace-imported files skipped",
                "only languages with named-import linking (JS/TS/Python)",
            ],
        },
    );
}

/// An untested `unused-export` finding narrowed to `names` (some of its
/// exports, in its order): its own id, count, severity and first line.
/// `None` when `names` is empty or its subject is not in the graph.
pub(super) fn regroup_unused_export(
    v: &View,
    finding: &Finding,
    names: &[&String],
) -> Option<Finding> {
    let file = v.t.by_key(&finding.subject)?;
    let line =
        v.t.node_ids()
            .zip(&v.t.nodes)
            .filter(|(_, node)| {
                node.kind == NodeKind::Symbol
                    && node.file == file
                    && node.parent == NONE
                    && node.flags & FLAG_EXPORTED != 0
                    && names.iter().any(|name| v.t.str(node.name) == name.as_str())
            })
            .map(|(_, node)| node.line)
            .min()?;
    let names = names.iter().map(|name| name.as_str()).collect::<Vec<_>>();
    let mut out = Vec::new();
    push_unused_export(
        &mut out,
        v,
        file,
        &names,
        line,
        finding.evidence["dynamicallyImported"] == true,
        false,
    );
    out.pop()
}

/// Who uses an export from another file.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Users {
    None,
    /// Test code only.
    Tests,
    Production,
}

/// The users of export `id` (declared in `file`) in other files. A
/// re-export statement records what a file makes public and is no use.
fn other_file_users(v: &View<'_>, id: u32, file: u32) -> Users {
    let t = v.t;
    let mut users = Users::None;
    for edge in t.incoming(id).filter(|e| {
        matches!(
            e.kind,
            EdgeKind::Uses | EdgeKind::Calls | EdgeKind::Inherits | EdgeKind::Passes
        ) && v.file_of[e.src as usize] != file
            && t.str(e.detail) != REEXPORTED_NAME
    }) {
        if !edge.test {
            return Users::Production;
        }
        users = Users::Tests;
    }
    users
}

/// A finding whose exports a test imports is a hypothesis: tier 60, as a
/// text mention makes it.
fn tier_tested(findings: &mut [Finding], tested: bool) {
    if let Some(finding) = findings.last_mut().filter(|_| tested) {
        finding.tier = finding.tier.min(60);
        finding
            .controls
            .push("a test imports it: un-exporting needs the test changed");
    }
}

/// States which files' exports were not judged, so an empty
/// `unused-export` or `export-only-local` result never reads as "none".
fn note_unjudged_exports(notes: &mut Vec<Value>, unjudged: &BTreeMap<&str, BTreeSet<u32>>) {
    if unjudged.is_empty() {
        return;
    }
    let files = unjudged.values().map(BTreeSet::len).sum::<usize>();
    let languages = unjudged
        .iter()
        .map(|(language, files)| format!("{language}: {}", files.len()))
        .collect::<Vec<_>>()
        .join(", ");
    let reason = format!(
        "exports of {files} production file(s) not judged ({languages}): only languages with named-import linking ({}) are checked; confirm with lspSearch references",
        NAMED_IMPORT_LANGUAGES.join(", ")
    );
    note_detectors(notes, &["unused-export", "export-only-local"], &reason);
}

/// An export run that found nothing says what it checked and which rule
/// settled each export; the entries whose re-exports made names public API
/// are named, for the lead to their re-exports.
fn note_settled_exports(notes: &mut Vec<Value>, v: &View<'_>, settled: &BTreeMap<&str, usize>) {
    let t = v.t;
    let mut evaluated = json!({"exports": settled.values().sum::<usize>()});
    for (rule, count) in settled {
        evaluated[*rule] = json!(count);
    }
    if settled.contains_key("reexportedByEntry") {
        let reexporting = v
            .entries
            .keys()
            .filter(|entry| {
                t.out(**entry).iter().any(|edge| {
                    edge.kind == EdgeKind::Imports && t.str(edge.detail).contains("reexport")
                })
            })
            .map(|entry| v.key(*entry))
            .collect::<Vec<_>>();
        evaluated["reexportingEntries"] = json!(reexporting);
    }
    note_evaluated(notes, &["unused-export", "export-only-local"], evaluated);
}

/// What a detector step checked when it found nothing: an empty answer is
/// never read as "nothing to check".
fn note_evaluated(notes: &mut Vec<Value>, detectors: &[&str], evaluated: Value) {
    notes.push(json!({"detectors": detectors, "evaluated": evaluated}));
}
