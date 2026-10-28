//! Reachability from entrypoints and unused-export detection.
use super::*;

/// Production files no entrypoint reaches: `(file, orphan)` rows, files
/// only tests reach, and the count of skipped Python library files.
pub(super) fn unreached_files(v: &View, entries: &[u32]) -> (Vec<(u32, bool)>, Vec<u32>, usize) {
    let prod = v.reach(entries, false);
    // Test seeds: test/config files plus test functions living inside
    // production files (Rust `mod tests`, pytest functions).
    let tests = v
        .files
        .iter()
        .copied()
        .filter(|f| v.flags(*f) & (ROLE_TEST | ROLE_CONFIG) != 0)
        .chain((0..v.t.nodes.len() as u32).filter(|id| {
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
    let entry_quality = if entries.len() >= 2 { 0.7 } else { 0.5 };
    let rules = v.entries.values().copied().collect::<BTreeSet<_>>();
    let scoped = v
        .files
        .iter()
        .filter(|f| v.production(**f) && v.package_scoped(**f))
        .count();
    if scoped > 0 {
        note(
            &mut c.notes,
            "unreachable-file",
            &format!(
                "{scoped} Go/JVM/.NET file(s) skipped: package-scoped files reference each other without imports"
            ),
        );
    }
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
                impact: 0.2,
                evidence: json!({"orphan": orphan, "entrypoints": entries.len(), "entryRules": rules}),
                controls,
                verify: vec![format!(
                    "octocode graph query dependents {} --edge imports,uses,calls",
                    quote(&key)
                )],
            },
        );
    }
    for file in test_only {
        let key = v.key(file).to_owned();
        push(
            &mut c.findings,
            Draft {
                detector: "test-only",
                subject: key.clone(),
                id_parts: &[],
                title: "Production file used only by tests".into(),
                severity: 0.45,
                confidence: entry_quality,
                impact: 0.2,
                evidence: json!({"reachedFromTests": true, "reachedFromEntrypoints": false}),
                controls: vec!["tests and configs treated as a separate entry class"],
                verify: vec![format!(
                    "octocode graph query dependents {} --edge imports,uses,calls",
                    quote(&key)
                )],
            },
        );
    }
}

/// Files whose exports are public API: entries and anything they
/// re-export, transitively.
pub(super) fn public_api_files(v: &View<'_>) -> BTreeSet<u32> {
    let t = v.t;
    let mut public = v.entries.keys().copied().collect::<BTreeSet<_>>();
    let mut queue = public.iter().copied().collect::<VecDeque<_>>();
    while let Some(file) = queue.pop_front() {
        for edge in t.out(file) {
            if edge.kind == EdgeKind::Imports
                && t.str(edge.detail).contains("reexport")
                && public.insert(edge.dst)
            {
                queue.push_back(edge.dst);
            }
        }
    }
    public
}

pub(super) fn detect_unused_exports(c: &mut Ctx) {
    let v = &c.v;
    let t = v.t;
    let public = public_api_files(v);
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
    let mut by_file = BTreeMap::<u32, Vec<&str>>::new();
    let mut local_only = BTreeMap::<u32, Vec<&str>>::new();
    // Files whose exports this detector cannot judge, by language.
    let mut unjudged = BTreeMap::<&str, BTreeSet<u32>>::new();
    for (id, node) in t.nodes.iter().enumerate() {
        if node.kind != NodeKind::Symbol || node.flags & FLAG_EXPORTED == 0 || node.parent != NONE {
            continue;
        }
        let file = node.file;
        // Judged languages are never package-scoped, so production is
        // file-level here; the language check below discloses the rest.
        if !v.production(file)
            || v.meta(file).contains(";library")
            || v.is_entry(file)
            || public.contains(&file)
            || namespace_used.contains(&file)
            || unreachable.contains(&file)
        {
            continue;
        }
        if !exports_judged(v.key(file), t.str(node.detail)) {
            if !NAMED_IMPORT_LANGUAGES.contains(&v.language(file)) {
                unjudged.entry(v.language(file)).or_default().insert(file);
            }
            continue;
        }
        let used = t.incoming(id as u32).any(|e| {
            matches!(
                e.kind,
                EdgeKind::Uses | EdgeKind::Calls | EdgeKind::Inherits | EdgeKind::Passes
            ) && v.file_of[e.src as usize] != file
        });
        if used {
            continue;
        }
        // Referenced inside its own file: the export is unnecessary, the
        // code is not dead (knip `ignoreExportsUsedInFile`).
        let local = node.flags & FLAG_LOCAL_USE != 0
            || t.incoming(id as u32).any(|e| {
                e.kind != EdgeKind::Contains
                    && v.file_of[e.src as usize] == file
                    && e.src != id as u32
            });
        let bucket = if local { &mut local_only } else { &mut by_file };
        bucket.entry(file).or_default().push(t.str(node.name));
    }
    note_unjudged_exports(&mut c.notes, &unjudged);
    for (file, names) in local_only {
        let key = v.key(file).to_owned();
        push(
            &mut c.findings,
            Draft {
                detector: "export-only-local",
                subject: key,
                id_parts: &[&names.join(",")],
                title: format!(
                    "{} export(s) used only inside this file: could be un-exported",
                    names.len()
                ),
                severity: 0.15,
                confidence: 0.8,
                impact: v.impact(file),
                evidence: json!({"exports": names, "count": names.len()}),
                controls: vec!["referenced within the file; not dead code"],
                verify: Vec::new(),
            },
        );
    }
    for (file, names) in by_file {
        let key = v.key(file).to_owned();
        let lazy = dynamic.contains(&file);
        push(
            &mut c.findings,
            Draft {
                detector: "unused-export",
                subject: key.clone(),
                id_parts: &[&names.join(",")],
                title: format!(
                    "{} export(s) with no importer outside the file",
                    names.len()
                ),
                severity: 0.4 * magnitude(names.len()),
                confidence: if lazy { 0.3 } else { 0.6 },
                impact: v.impact(file),
                evidence: json!({"exports": names, "count": names.len(), "dynamicallyImported": lazy}),
                controls: vec![
                    "entry files and everything they re-export treated as public API",
                    "namespace-imported files skipped",
                    "only languages with named-import linking (JS/TS/Python)",
                ],
                verify: vec![format!(
                    "octocode lspSearch '{{\"queries\":[{{\"operation\":\"references\",\"path\":\"{key}\",\"symbolName\":\"{}\"}}]}}'",
                    names[0]
                )],
            },
        );
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
    for detector in ["unused-export", "export-only-local"] {
        note(notes, detector, &reason);
    }
}
