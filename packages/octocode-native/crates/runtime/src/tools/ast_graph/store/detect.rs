//! `graph query issues`: graph-algorithm detectors that rank *possible*
//! problems. Every finding is a hypothesis carrying its evidence, the
//! false-positive controls applied, and commands that verify it.
//!
//! Two rules keep findings honest:
//! - **Liveness is optimistic.** Any edge (a low-confidence call, a dynamic or
//!   type-only import) keeps a node alive for dead-code detectors.
//! - **Violations are pessimistic.** Only runtime import edges between
//!   authored files count as evidence for cycles, hubs, and layering.
//!
//! Generated, bundled, vendored, and declaration files are never subjects.
use super::classify::{
    ROLE_CONFIG, ROLE_DECLARATION, ROLE_ENTRY, ROLE_NOT_AUTHORED, ROLE_TEST, ROLE_UNPARSED,
    role_names,
};
use super::format::{
    EdgeKind, FLAG_EXPORTED, FLAG_LOCAL_USE, FLAG_TEST, GraphTables, NONE, NodeKind,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub(crate) const DETECTORS: &[&str] = &[
    "cycle",
    "dir-cycle",
    "unreachable-file",
    "test-only",
    "unused-export",
    "export-only-local",
    "undeclared-dependency",
    "dev-dependency-in-production",
    "boundary-violation",
    "unresolved-import",
    "god-file",
    "critical-file",
    "single-point-of-failure",
    "unstable-dependency",
    "main-sequence",
    "misplaced-file",
];

/// Languages whose named imports are linked to declarations, so "no
/// importer" is meaningful for unused-export.
const NAMED_IMPORT_LANGUAGES: &[&str] = &["typescript", "tsx", "javascript", "jsx", "python"];
const PACKAGE_SCOPED: &[&str] = &["go", "java", "kotlin", "scala", "csharp"];
const ABSTRACT_KINDS: &[&str] = &[
    "interface",
    "trait",
    "type",
    "type_alias",
    "typeAlias",
    "protocol",
];
const MAX_EVIDENCE: usize = 20;

#[derive(Clone, Debug)]
pub(crate) struct Finding {
    pub detector: &'static str,
    pub subject: String,
    /// Stable across snapshots: detector + subject + identifying evidence.
    pub id: String,
    pub title: String,
    pub severity: f64,
    pub confidence: f64,
    pub impact: f64,
    pub score: f64,
    pub evidence: Value,
    pub controls: Vec<&'static str>,
    pub verify: Vec<String>,
    pub corroborated_by: Vec<&'static str>,
    /// vulture-style certainty: 100 nothing references it anywhere, 90 no
    /// graph or text reference, 60 referenced by text only (strings,
    /// configs) or otherwise uncertain. The default view shows 90+.
    pub tier: u8,
}

impl Finding {
    pub(crate) fn to_json(&self) -> Value {
        let round = |v: f64| (v * 1000.0).round() / 1000.0;
        let mut out = json!({
            "id": self.id,
            "detector": self.detector,
            "subject": self.subject,
            "title": self.title,
            "score": round(self.score),
            "severity": round(self.severity),
            "confidence": round(self.confidence),
            "impact": round(self.impact),
            "tier": self.tier,
            "evidence": self.evidence,
        });
        if !self.controls.is_empty() {
            out["controls"] = json!(self.controls);
        }
        if !self.corroborated_by.is_empty() {
            out["corroboratedBy"] = json!(self.corroborated_by);
        }
        if !self.verify.is_empty() {
            out["verify"] = json!(self.verify);
        }
        out
    }
}

pub(crate) struct Report {
    pub findings: Vec<Finding>,
    pub summary: Value,
}

fn stable_id(detector: &str, parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(detector.as_bytes());
    for part in parts {
        hasher.update([0]);
        hasher.update(part.as_bytes());
    }
    hex::encode(hasher.finalize())[..12].to_owned()
}

/// Maps magnitude (members, dependents, count) into a `[0.6, 1.0]` factor.
fn magnitude(size: usize) -> f64 {
    (0.6 + 0.08 * ((1 + size) as f64).log2()).min(1.0)
}

fn quote(arg: &str) -> String {
    if arg
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "-_./:@".contains(c))
    {
        arg.to_owned()
    } else {
        format!("'{}'", arg.replace('\'', r"'\''"))
    }
}

fn parent_dir(path: &str) -> &str {
    path.rsplit_once('/').map_or(".", |(dir, _)| dir)
}

/// Precomputed file-level views shared by every detector.
struct View<'a> {
    t: &'a GraphTables,
    /// File node ids in key order.
    files: Vec<u32>,
    /// Node → owning file node.
    file_of: Vec<u32>,
    /// Runtime import adjacency between authored files (sorted, distinct).
    out: Vec<Vec<u32>>,
    inc: Vec<Vec<u32>>,
    /// First importing line per runtime edge.
    edge_line: BTreeMap<(u32, u32), u32>,
    /// File → (component dir, name, meta `ecosystem[;library][;templates]`).
    component: BTreeMap<u32, (&'a str, &'a str, &'a str)>,
    entries: BTreeMap<u32, &'a str>,
    rank: Vec<f64>,
    rank_percentile: Vec<f64>,
}

impl<'a> View<'a> {
    fn new(t: &'a GraphTables) -> Self {
        let n = t.nodes.len();
        let files = (0..n as u32)
            .filter(|id| t.nodes[*id as usize].kind == NodeKind::File)
            .collect::<Vec<_>>();
        let file_of = t.nodes.iter().map(|node| node.file).collect::<Vec<_>>();
        let mut out = vec![Vec::new(); n];
        let mut inc = vec![Vec::new(); n];
        let mut edge_line = BTreeMap::new();
        for edge in &t.edges {
            if edge.kind != EdgeKind::Imports || edge.src == edge.dst {
                continue;
            }
            let (src, dst) = (&t.nodes[edge.src as usize], &t.nodes[edge.dst as usize]);
            if src.kind != NodeKind::File || dst.kind != NodeKind::File {
                continue;
            }
            if (src.flags | dst.flags) & (ROLE_NOT_AUTHORED | ROLE_DECLARATION) != 0 {
                continue;
            }
            let detail = t.str(edge.detail);
            // Type-only and lazily loaded edges never create a runtime cycle.
            if detail.contains("type") || detail == "dynamic-import" || detail == "lazy-import" {
                continue;
            }
            out[edge.src as usize].push(edge.dst);
            inc[edge.dst as usize].push(edge.src);
            let line = edge_line.entry((edge.src, edge.dst)).or_insert(edge.line);
            *line = (*line).min(edge.line);
        }
        for list in out.iter_mut().chain(inc.iter_mut()) {
            list.sort_unstable();
            list.dedup();
        }
        let component = t
            .components
            .iter()
            .map(|(node, dir, name, meta)| (*node, (t.str(*dir), t.str(*name), t.str(*meta))))
            .collect();
        let entries = t
            .entries
            .iter()
            .map(|(node, rule)| (*node, t.str(*rule)))
            .collect();
        let mut view = Self {
            t,
            files,
            file_of,
            out,
            inc,
            edge_line,
            component,
            entries,
            rank: Vec::new(),
            rank_percentile: Vec::new(),
        };
        view.rank = view.page_rank();
        view.rank_percentile = view.percentiles(&view.rank);
        view
    }

    fn key(&self, id: u32) -> &'a str {
        self.t.str(self.t.nodes[id as usize].key)
    }
    fn flags(&self, file: u32) -> u8 {
        self.t.nodes[file as usize].flags
    }
    fn language(&self, file: u32) -> &'a str {
        self.t.str(self.t.nodes[file as usize].detail)
    }
    fn authored(&self, file: u32) -> bool {
        // Unparsed (oversized) files have no facts to judge.
        self.flags(file) & (ROLE_NOT_AUTHORED | ROLE_DECLARATION | ROLE_UNPARSED) == 0
    }
    /// A file that may be a subject of a production-code finding.
    /// Go/JVM/.NET files share a package scope without importing each
    /// other, so file-level structure (reachability, hubs, file cycles) is
    /// not observable for them; package-level detectors still apply.
    fn package_scoped(&self, file: u32) -> bool {
        PACKAGE_SCOPED.contains(&self.language(file))
    }
    /// A production file whose file-level structure the graph observes.
    fn file_level(&self, file: u32) -> bool {
        self.production(file) && !self.package_scoped(file)
    }
    fn meta(&self, file: u32) -> &'a str {
        self.component.get(&file).map_or("", |(_, _, meta)| *meta)
    }
    fn production(&self, file: u32) -> bool {
        self.authored(file) && self.flags(file) & (ROLE_TEST | ROLE_CONFIG) == 0
    }
    fn impact(&self, file: u32) -> f64 {
        self.rank_percentile
            .get(file as usize)
            .copied()
            .unwrap_or(0.5)
    }
    fn line(&self, src: u32, dst: u32) -> Option<u32> {
        self.edge_line
            .get(&(src, dst))
            .copied()
            .filter(|l| *l != NONE)
    }

    /// PageRank over runtime imports (importer → imported): a file many
    /// important files depend on ranks high. Damping 0.85, ≤ 50 iterations.
    fn page_rank(&self) -> Vec<f64> {
        let n = self.t.nodes.len();
        let count = self.files.len().max(1) as f64;
        let mut rank = vec![0.0; n];
        for file in &self.files {
            rank[*file as usize] = 1.0 / count;
        }
        for _ in 0..50 {
            let mut next = vec![0.0; n];
            let mut dangling = 0.0;
            for file in &self.files {
                let targets = &self.out[*file as usize];
                let r = rank[*file as usize];
                if targets.is_empty() {
                    dangling += r;
                } else {
                    let share = r / targets.len() as f64;
                    for target in targets {
                        next[*target as usize] += share;
                    }
                }
            }
            let base = (1.0 - 0.85) / count + 0.85 * dangling / count;
            let mut delta = 0.0;
            for file in &self.files {
                let value = base + 0.85 * next[*file as usize];
                delta += (value - rank[*file as usize]).abs();
                next[*file as usize] = value;
            }
            rank = next;
            if delta < 1e-6 {
                break;
            }
        }
        rank
    }

    fn percentiles(&self, values: &[f64]) -> Vec<f64> {
        let mut order = self.files.clone();
        order.sort_by(|a, b| {
            values[*a as usize]
                .total_cmp(&values[*b as usize])
                .then(a.cmp(b))
        });
        let mut out = vec![0.5; self.t.nodes.len()];
        let denominator = (order.len().max(2) - 1) as f64;
        for (position, file) in order.iter().enumerate() {
            out[*file as usize] = position as f64 / denominator;
        }
        out
    }

    /// Files transitively depending on `file` through runtime imports.
    fn dependents(&self, file: u32) -> usize {
        let mut seen = BTreeSet::from([file]);
        let mut queue = VecDeque::from([file]);
        while let Some(id) = queue.pop_front() {
            for next in &self.inc[id as usize] {
                if seen.insert(*next) {
                    queue.push_back(*next);
                }
            }
        }
        seen.len() - 1
    }

    /// Node-level forward reachability over every edge kind from `seeds`;
    /// reaching a symbol also reaches (loads) its file.
    fn reach(&self, seeds: &[u32]) -> Vec<bool> {
        let mut seen = vec![false; self.t.nodes.len()];
        let mut queue = VecDeque::new();
        for seed in seeds {
            if !seen[*seed as usize] {
                seen[*seed as usize] = true;
                queue.push_back(*seed);
            }
        }
        while let Some(id) = queue.pop_front() {
            let file = self.file_of[id as usize];
            let mut visit = |next: u32, queue: &mut VecDeque<u32>| {
                if next != NONE && !seen[next as usize] {
                    seen[next as usize] = true;
                    queue.push_back(next);
                }
            };
            visit(file, &mut queue);
            for edge in self.t.out(id) {
                visit(edge.dst, &mut queue);
            }
        }
        seen
    }
}

// ── Graph primitives ────────────────────────────────────────────────────────

/// Iterative Tarjan over `nodes` using `adj`; components of size ≥ 2.
fn strongly_connected(nodes: &[u32], adj: &dyn Fn(u32) -> Vec<u32>, n: usize) -> Vec<Vec<u32>> {
    let mut index = vec![u32::MAX; n];
    let mut low = vec![0u32; n];
    let mut on_stack = vec![false; n];
    let mut stack = Vec::new();
    let mut components = Vec::new();
    let mut counter = 0u32;
    for &start in nodes {
        if index[start as usize] != u32::MAX {
            continue;
        }
        let mut frames = vec![(start, adj(start), 0usize)];
        index[start as usize] = counter;
        low[start as usize] = counter;
        counter += 1;
        stack.push(start);
        on_stack[start as usize] = true;
        while let Some((id, next, at)) = frames.last_mut() {
            let id = *id;
            if let Some(&w) = next.get(*at) {
                *at += 1;
                if index[w as usize] == u32::MAX {
                    index[w as usize] = counter;
                    low[w as usize] = counter;
                    counter += 1;
                    stack.push(w);
                    on_stack[w as usize] = true;
                    frames.push((w, adj(w), 0));
                } else if on_stack[w as usize] {
                    low[id as usize] = low[id as usize].min(index[w as usize]);
                }
                continue;
            }
            frames.pop();
            if let Some((parent, _, _)) = frames.last() {
                low[*parent as usize] = low[*parent as usize].min(low[id as usize]);
            }
            if low[id as usize] == index[id as usize] {
                let mut component = Vec::new();
                while let Some(w) = stack.pop() {
                    on_stack[w as usize] = false;
                    component.push(w);
                    if w == id {
                        break;
                    }
                }
                if component.len() > 1 {
                    component.sort_unstable();
                    components.push(component);
                }
            }
        }
    }
    components
}

/// Eades–Lin–Smyth greedy feedback-arc set inside one SCC: the returned
/// edges, if removed, leave the members acyclic (|FAS| ≤ m/2 − n/6).
/// Degrees are maintained incrementally, so each pick is O(n) and the whole
/// ordering O(n² + m).
fn feedback_arcs(members: &[u32], adj: &dyn Fn(u32) -> Vec<u32>) -> Vec<(u32, u32)> {
    let index = members
        .iter()
        .enumerate()
        .map(|(i, m)| (*m, i))
        .collect::<BTreeMap<_, _>>();
    let k = members.len();
    let mut succ = vec![Vec::new(); k];
    let mut pred = vec![Vec::new(); k];
    for (i, m) in members.iter().enumerate() {
        for to in adj(*m) {
            if let Some(&j) = index.get(&to)
                && j != i
            {
                succ[i].push(j);
                pred[j].push(i);
            }
        }
    }
    let mut out_deg = succ.iter().map(|s| s.len() as i64).collect::<Vec<_>>();
    let mut in_deg = pred.iter().map(|p| p.len() as i64).collect::<Vec<_>>();
    let mut alive = vec![true; k];
    let mut left_count = 0usize;
    let mut position = vec![0usize; k];
    let mut right = Vec::new();
    let mut remaining = k;
    let remove =
        |node: usize, alive: &mut Vec<bool>, out_deg: &mut Vec<i64>, in_deg: &mut Vec<i64>| {
            alive[node] = false;
            for &p in &pred[node] {
                if alive[p] {
                    out_deg[p] -= 1;
                }
            }
            for &s in &succ[node] {
                if alive[s] {
                    in_deg[s] -= 1;
                }
            }
        };
    while remaining > 0 {
        let mut progressed = true;
        while progressed {
            progressed = false;
            for node in 0..k {
                if !alive[node] {
                    continue;
                }
                if out_deg[node] == 0 {
                    right.push(node);
                } else if in_deg[node] == 0 {
                    position[node] = left_count;
                    left_count += 1;
                } else {
                    continue;
                }
                remove(node, &mut alive, &mut out_deg, &mut in_deg);
                remaining -= 1;
                progressed = true;
            }
        }
        if let Some(best) = (0..k)
            .filter(|n| alive[*n])
            .max_by_key(|n| (out_deg[*n] - in_deg[*n], std::cmp::Reverse(*n)))
        {
            position[best] = left_count;
            left_count += 1;
            remove(best, &mut alive, &mut out_deg, &mut in_deg);
            remaining -= 1;
        }
    }
    for (offset, node) in right.iter().rev().enumerate() {
        position[*node] = left_count + offset;
    }
    let mut cuts = Vec::new();
    for (from, targets) in succ.iter().enumerate() {
        for to in targets {
            if position[from] > position[*to] {
                cuts.push((members[from], members[*to]));
            }
        }
    }
    cuts.sort_unstable();
    cuts.dedup();
    cuts
}

/// Shortest directed cycle through `start` inside `members`.
fn witness_cycle(start: u32, members: &BTreeSet<u32>, adj: &dyn Fn(u32) -> Vec<u32>) -> Vec<u32> {
    let mut previous = BTreeMap::new();
    let mut queue = VecDeque::from([start]);
    let mut seen = BTreeSet::from([start]);
    while let Some(id) = queue.pop_front() {
        for next in adj(id).into_iter().filter(|x| members.contains(x)) {
            if next == start {
                let mut path = vec![id];
                let mut cursor = id;
                while let Some(prev) = previous.get(&cursor) {
                    path.push(*prev);
                    cursor = *prev;
                }
                path.reverse();
                path.push(start);
                return path;
            }
            if seen.insert(next) {
                previous.insert(next, id);
                queue.push_back(next);
            }
        }
    }
    vec![start]
}

/// Articulation points of the undirected projection with, for each, the
/// size of the smaller side it separates.
fn articulation_points(v: &View) -> Vec<(u32, usize)> {
    let n = v.t.nodes.len();
    let neighbors = |id: u32| {
        let mut all = v.out[id as usize].clone();
        all.extend(&v.inc[id as usize]);
        all.sort_unstable();
        all.dedup();
        all
    };
    let mut disc = vec![u32::MAX; n];
    let mut low = vec![0u32; n];
    let mut size = vec![1usize; n];
    let mut timer = 0u32;
    let mut result = Vec::new();
    for &root in &v.files {
        if disc[root as usize] != u32::MAX || neighbors(root).is_empty() {
            continue;
        }
        // First pass: component size.
        let mut comp = 0usize;
        {
            let mut seen = BTreeSet::from([root]);
            let mut queue = VecDeque::from([root]);
            while let Some(id) = queue.pop_front() {
                comp += 1;
                for next in neighbors(id) {
                    if seen.insert(next) {
                        queue.push_back(next);
                    }
                }
            }
        }
        let mut separated: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        let mut root_children = 0usize;
        disc[root as usize] = timer;
        low[root as usize] = timer;
        timer += 1;
        let mut frames = vec![(root, u32::MAX, neighbors(root), 0usize)];
        while let Some((id, parent, next, at)) = frames.last_mut() {
            let (id, parent) = (*id, *parent);
            if let Some(&w) = next.get(*at) {
                *at += 1;
                if disc[w as usize] == u32::MAX {
                    disc[w as usize] = timer;
                    low[w as usize] = timer;
                    timer += 1;
                    if id == root {
                        root_children += 1;
                    }
                    frames.push((w, id, neighbors(w), 0));
                } else if w != parent {
                    low[id as usize] = low[id as usize].min(disc[w as usize]);
                }
                continue;
            }
            frames.pop();
            if let Some((p, _, _, _)) = frames.last() {
                let p = *p;
                size[p as usize] += size[id as usize];
                low[p as usize] = low[p as usize].min(low[id as usize]);
                if low[id as usize] >= disc[p as usize] {
                    separated.entry(p).or_default().push(size[id as usize]);
                }
            }
        }
        for (point, pieces) in separated {
            if point == root && root_children < 2 {
                continue;
            }
            let largest = pieces.iter().copied().max().unwrap_or(0);
            let rest = comp.saturating_sub(1 + largest);
            let smaller = if point == root {
                pieces.iter().copied().sum::<usize>() - largest
            } else {
                largest.min(rest)
            };
            result.push((point, smaller));
        }
    }
    result
}

// ── Detectors ───────────────────────────────────────────────────────────────

/// `(timing label, detectors it produces, runner)`.
type DetectorStep = (&'static str, &'static [&'static str], fn(&mut Ctx));

struct Ctx<'a> {
    v: View<'a>,
    findings: Vec<Finding>,
    notes: Vec<Value>,
}

#[allow(clippy::too_many_arguments)]
fn push(
    out: &mut Vec<Finding>,
    detector: &'static str,
    subject: String,
    id_parts: &[&str],
    title: String,
    severity: f64,
    confidence: f64,
    impact: f64,
    evidence: Value,
    controls: Vec<&'static str>,
    verify: Vec<String>,
) {
    let mut parts = vec![subject.as_str()];
    parts.extend(id_parts);
    let id = stable_id(detector, &parts);
    out.push(Finding {
        detector,
        id,
        subject,
        title,
        severity,
        confidence,
        impact,
        score: severity * confidence * (0.5 + 0.5 * impact),
        evidence,
        controls,
        verify,
        corroborated_by: Vec::new(),
        tier: 90,
    });
}

fn note(notes: &mut Vec<Value>, detector: &str, reason: &str) {
    notes.push(json!({"detector": detector, "skipped": reason}));
}

fn detect_cycles(c: &mut Ctx) {
    let v = &c.v;
    let adj = |id: u32| v.out[id as usize].clone();
    // Go forbids import cycles between packages; file-level SCCs inside one
    // package are normal.
    let nodes = v
        .files
        .iter()
        .copied()
        .filter(|f| v.file_level(*f))
        .collect::<Vec<_>>();
    let allowed = nodes.iter().copied().collect::<BTreeSet<_>>();
    let restricted = |id: u32| {
        adj(id)
            .into_iter()
            .filter(|x| allowed.contains(x))
            .collect::<Vec<_>>()
    };
    let components = strongly_connected(&nodes, &restricted, v.t.nodes.len());
    let mut found = Vec::new();
    for members in components {
        let set = members.iter().copied().collect::<BTreeSet<_>>();
        let witness = witness_cycle(members[0], &set, &restricted);
        let cuts = feedback_arcs(&members, &restricted);
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
            "members": member_keys.iter().take(MAX_EVIDENCE).collect::<Vec<_>>(),
            "witness": witness.windows(2).map(|w| hop(w[0], w[1])).collect::<Vec<_>>(),
            "suggestedCuts": cuts.iter().take(MAX_EVIDENCE).map(|(a, b)| hop(*a, *b)).collect::<Vec<_>>(),
            "cutCount": cuts.len(),
        });
        let mut controls = vec!["type-only imports excluded", "dynamic imports excluded"];
        let mut confidence = 0.85;
        if rust {
            confidence = 0.35;
            controls.push("rust modules within one crate may legally cycle");
        }
        let first = v.key(witness[0]);
        let second = witness.get(1).map_or(first, |id| v.key(*id));
        found.push((
            member_keys[0].to_owned(),
            member_keys.join(","),
            format!(
                "Runtime import cycle of {} files; {} import(s) break it",
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
            "cycle",
            subject,
            &[&all],
            title,
            severity,
            confidence,
            impact,
            evidence,
            controls,
            verify,
        );
    }
}

fn detect_dir_cycles(c: &mut Ctx) {
    let v = &c.v;
    let mut dirs = BTreeMap::<&str, u32>::new();
    let mut names = Vec::new();
    for file in &v.files {
        if v.production(*file) && v.language(*file) != "go" {
            let dir = parent_dir(v.key(*file));
            if !dirs.contains_key(dir) {
                dirs.insert(dir, names.len() as u32);
                names.push(dir);
            }
        }
    }
    let mut adj = vec![BTreeMap::<u32, (u32, u32)>::new(); names.len()];
    for file in &v.files {
        let Some(&from) = dirs.get(parent_dir(v.key(*file))) else {
            continue;
        };
        for target in &v.out[*file as usize] {
            if let Some(&to) = dirs.get(parent_dir(v.key(*target)))
                && from != to
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
            "dir-cycle",
            keys[0].to_owned(),
            &[&keys.join(",")],
            format!(
                "Directories depend on each other in a cycle ({} dirs)",
                keys.len()
            ),
            0.5 * magnitude(keys.len()),
            if rust { 0.3 } else { 0.7 },
            0.5,
            json!({"dirs": keys.iter().take(MAX_EVIDENCE).collect::<Vec<_>>(), "witness": hops}),
            controls,
            Vec::new(),
        );
    }
}

fn detect_reachability(c: &mut Ctx) {
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
    let prod = v.reach(&entries);
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
    let test_reach = v.reach(&tests);
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
            "unreachable-file",
            key.clone(),
            &[],
            if orphan {
                "Orphan file: nothing imports it and it imports nothing".into()
            } else {
                "Not reachable from any inferred entrypoint".into()
            },
            0.6,
            entry_quality * if orphan { 1.0 } else { 0.85 } * if templates { 0.5 } else { 1.0 },
            0.2,
            json!({"orphan": orphan, "entrypoints": entries.len(), "entryRules": rules}),
            controls,
            vec![
                format!(
                    "octocode graph query dependents {} --edge imports,uses,calls",
                    quote(&key)
                ),
                "confirm with lspSearch references on its exports before deleting".into(),
            ],
        );
    }
    for file in test_only {
        let key = v.key(file).to_owned();
        push(
            &mut c.findings,
            "test-only",
            key.clone(),
            &[],
            "Production file used only by tests".into(),
            0.45,
            entry_quality,
            0.2,
            json!({"reachedFromTests": true, "reachedFromEntrypoints": false}),
            vec!["tests and configs treated as a separate entry class"],
            vec![format!(
                "octocode graph query dependents {} --edge imports,uses,calls",
                quote(&key)
            )],
        );
    }
}

fn detect_unused_exports(c: &mut Ctx) {
    let v = &c.v;
    let t = v.t;
    // Files whose exports are public API: entries and anything they
    // re-export, transitively.
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
    for (id, node) in t.nodes.iter().enumerate() {
        if node.kind != NodeKind::Symbol || node.flags & FLAG_EXPORTED == 0 || node.parent != NONE {
            continue;
        }
        let file = node.file;
        if !v.file_level(file)
            || v.meta(file).contains(";library")
            || v.flags(file) & ROLE_ENTRY != 0
            || public.contains(&file)
            || namespace_used.contains(&file)
            || !NAMED_IMPORT_LANGUAGES.contains(&v.language(file))
        {
            continue;
        }
        // Python has no export syntax: module-level variables in scripts
        // are not API. Only functions and classes count.
        if v.language(file) == "python" && !matches!(t.str(node.detail), "function" | "class") {
            continue;
        }
        let used = t.incoming(id as u32).any(|e| {
            matches!(
                e.kind,
                EdgeKind::Uses | EdgeKind::Calls | EdgeKind::Inherits
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
    for (file, names) in local_only {
        let key = v.key(file).to_owned();
        push(
            &mut c.findings,
            "export-only-local",
            key,
            &[&names.join(",")],
            format!(
                "{} export(s) used only inside this file: could be un-exported",
                names.len()
            ),
            0.15,
            0.8,
            v.impact(file),
            json!({"exports": names.iter().take(MAX_EVIDENCE).collect::<Vec<_>>(), "count": names.len()}),
            vec!["referenced within the file; not dead code"],
            Vec::new(),
        );
    }
    for (file, names) in by_file {
        let key = v.key(file).to_owned();
        let lazy = dynamic.contains(&file);
        push(
            &mut c.findings,
            "unused-export",
            key.clone(),
            &[&names.join(",")],
            format!(
                "{} export(s) with no importer outside the file",
                names.len()
            ),
            0.4 * magnitude(names.len()),
            if lazy { 0.3 } else { 0.6 },
            v.impact(file),
            json!({"exports": names.iter().take(MAX_EVIDENCE).collect::<Vec<_>>(), "count": names.len(), "dynamicallyImported": lazy}),
            vec![
                "entry files and everything they re-export treated as public API",
                "namespace-imported files skipped",
                "only languages with named-import linking (JS/TS/Python)",
            ],
            vec![format!(
                "octocode lspSearch '{{\"queries\":[{{\"operation\":\"references\",\"uri\":\"{key}\",\"symbolName\":\"{}\"}}]}}'",
                names[0]
            )],
        );
    }
}

fn detect_dependencies(c: &mut Ctx) {
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
            .take(5)
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
    for (detector, subject, title, severity, confidence, evidence, controls) in pending {
        push(
            &mut c.findings,
            detector,
            subject,
            &[],
            title,
            severity,
            confidence,
            0.5,
            evidence,
            controls,
            Vec::new(),
        );
    }
}

fn detect_boundaries(c: &mut Ctx) {
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
            if npm && from != to && v.flags(*target) & ROLE_ENTRY == 0 && entries_of(to) > 0 {
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
            "sites": pairs.iter().take(5).map(|(a, b)| json!({"from": v.key(*a), "to": v.key(*b), "line": v.line(*a, *b)})).collect::<Vec<_>>(),
        });
        pending.push((format!("{from} -> {to}"), pairs.len(), evidence, pairs[0].1));
    }
    for (subject, count, evidence, target) in pending {
        push(
            &mut c.findings,
            "boundary-violation",
            subject,
            &[],
            format!("Deep import into another package's internals ({count} import(s))"),
            0.45 * magnitude(count),
            0.6,
            c.v.impact(target),
            evidence,
            vec!["targets that are package entrypoints/exports are allowed"],
            Vec::new(),
        );
    }
}

fn detect_unresolved(c: &mut Ctx) {
    let t = c.v.t;
    let mut by_file = BTreeMap::<&str, Vec<Value>>::new();
    for diag in &t.diagnostics {
        if t.str(diag.code) == "unresolved-internal" {
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
            "unresolved-import",
            file.clone(),
            &[],
            format!("{} internal import(s) could not be resolved", sites.len()),
            0.5 * magnitude(sites.len()),
            0.8,
            0.5,
            json!({"count": sites.len(), "sites": sites.into_iter().take(MAX_EVIDENCE).collect::<Vec<_>>()}),
            vec!["relative/crate-internal specifiers only; packages are never unresolved"],
            vec![format!("octocode graph query diagnostics {}", quote(&file))],
        );
    }
}

fn median(mut values: Vec<usize>) -> usize {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    values[values.len() / 2]
}

fn is_barrel(v: &View, file: u32) -> bool {
    let reexports =
        v.t.out(file)
            .iter()
            .filter(|e| e.kind == EdgeKind::Imports && v.t.str(e.detail).contains("reexport"))
            .count();
    let declarations =
        v.t.out(file)
            .iter()
            .filter(|e| {
                e.kind == EdgeKind::Contains && v.t.nodes[e.dst as usize].kind == NodeKind::Symbol
            })
            .count();
    reexports >= 3 && declarations <= 1
}

fn detect_hubs(c: &mut Ctx) {
    let v = &c.v;
    let candidates = v
        .files
        .iter()
        .copied()
        .filter(|f| v.file_level(*f))
        .collect::<Vec<_>>();
    let fan_in = |f: u32| v.inc[f as usize].len();
    let fan_out = |f: u32| v.out[f as usize].len();
    let mid_in = median(
        candidates
            .iter()
            .map(|f| fan_in(*f))
            .filter(|x| *x > 0)
            .collect(),
    );
    let mid_out = median(
        candidates
            .iter()
            .map(|f| fan_out(*f))
            .filter(|x| *x > 0)
            .collect(),
    );
    let mut pending = Vec::new();
    for file in &candidates {
        let (i, o) = (fan_in(*file), fan_out(*file));
        let balanced = (i as i64 - o as i64).unsigned_abs() as f64 * 4.0 < (i + o) as f64;
        if i > mid_in.max(2)
            && o > mid_out.max(2)
            && balanced
            && i + o >= 10
            && !is_barrel(v, *file)
        {
            pending.push((*file, i, o));
        }
    }
    for (file, i, o) in pending {
        let dependents = c.v.dependents(file);
        push(
            &mut c.findings,
            "god-file",
            c.v.key(file).to_owned(),
            &[],
            format!("Hub-like file: {i} importers and {o} imports"),
            0.5 * magnitude(i * o / 4),
            0.7,
            c.v.impact(file),
            json!({"fanIn": i, "fanOut": o, "medianFanIn": mid_in, "medianFanOut": mid_out, "transitiveDependents": dependents}),
            vec![
                "Arcan hub-like thresholds (fan-in/out above median, balanced)",
                "barrels excluded",
            ],
            vec![format!(
                "octocode graph query symbols {}",
                quote(c.v.key(file))
            )],
        );
    }
}

fn detect_critical(c: &mut Ctx) {
    let v = &c.v;
    let mut ranked = v
        .files
        .iter()
        .copied()
        .filter(|f| v.file_level(*f) && !v.inc[*f as usize].is_empty())
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| {
        v.rank[*b as usize]
            .total_cmp(&v.rank[*a as usize])
            .then(a.cmp(b))
    });
    let take = (v.files.len() / 50).clamp(1, 10);
    let pending = ranked.into_iter().take(take).collect::<Vec<_>>();
    for file in pending {
        let dependents = c.v.dependents(file);
        if dependents < 5 {
            continue;
        }
        push(
            &mut c.findings,
            "critical-file",
            c.v.key(file).to_owned(),
            &[],
            format!("High blast radius: {dependents} files depend on it transitively"),
            0.3,
            0.9,
            c.v.impact(file),
            json!({"pageRank": c.v.rank[file as usize], "transitiveDependents": dependents, "directImporters": c.v.inc[file as usize].len()}),
            vec!["informational: changes here need wide test coverage"],
            vec![format!(
                "octocode graph query dependents {} --depth 3",
                quote(c.v.key(file))
            )],
        );
    }
}

fn detect_spof(c: &mut Ctx) {
    let points = articulation_points(&c.v);
    for (file, smaller) in points {
        if smaller < 5 || !c.v.file_level(file) {
            continue;
        }
        push(
            &mut c.findings,
            "single-point-of-failure",
            c.v.key(file).to_owned(),
            &[],
            format!(
                "Only link between two parts of the import graph ({smaller} files on the smaller side)"
            ),
            0.4 * magnitude(smaller),
            0.6,
            c.v.impact(file),
            json!({"smallerSide": smaller}),
            vec!["undirected runtime import projection"],
            vec![format!(
                "octocode graph query walk {} --direction both --edge imports --depth 1",
                quote(c.v.key(file))
            )],
        );
    }
}

/// Component-level Martin metrics: `unstable-dependency`, `main-sequence`.
fn detect_components(c: &mut Ctx) {
    let v = &c.v;
    let t = v.t;
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
            parent_dir(v.key(file)).to_owned()
        }
    };
    let level = if use_components {
        "package"
    } else {
        "directory"
    };
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
    let instability = |name: &str| {
        let ce = efferent.get(name).map_or(0, BTreeSet::len) as f64;
        let ca = afferent.get(name).map_or(0, BTreeSet::len) as f64;
        if ca + ce == 0.0 { 0.0 } else { ce / (ca + ce) }
    };
    // Abstractness from exported symbol kinds.
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
                       "lessStable": worse.iter().take(MAX_EVIDENCE).map(|d| json!({"unit": d, "instability": (instability(d) * 1000.0).round() / 1000.0})).collect::<Vec<_>>()}),
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
    for (detector, subject, title, severity, confidence, evidence, controls) in pending {
        push(
            &mut c.findings,
            detector,
            subject,
            &[],
            title,
            severity,
            confidence,
            0.5,
            evidence,
            controls,
            Vec::new(),
        );
    }
}

fn detect_misplaced(c: &mut Ctx) {
    let v = &c.v;
    let mut pending = Vec::new();
    for file in &v.files {
        let key = v.key(*file);
        let name = key.rsplit('/').next().unwrap_or(key);
        let stem = name.split('.').next().unwrap_or(name);
        if !v.file_level(*file)
            || v.flags(*file) & ROLE_ENTRY != 0
            || matches!(stem, "index" | "mod" | "__init__" | "lib" | "main")
            || is_barrel(v, *file)
        {
            continue;
        }
        let home = parent_dir(key);
        let mut by_dir = BTreeMap::<&str, usize>::new();
        let mut neighbors = v.out[*file as usize].clone();
        neighbors.extend(&v.inc[*file as usize]);
        neighbors.sort_unstable();
        neighbors.dedup();
        for other in &neighbors {
            *by_dir.entry(parent_dir(v.key(*other))).or_default() += 1;
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
        {
            pending.push((*file, (*dir).to_owned(), *count, same, total));
        }
    }
    for (file, dir, count, same, total) in pending {
        push(
            &mut c.findings,
            "misplaced-file",
            c.v.key(file).to_owned(),
            &[],
            format!("Most of its dependencies live in {dir}"),
            0.25,
            0.5,
            c.v.impact(file),
            json!({"suggestedDir": dir, "edgesToSuggested": count, "edgesToOwnDir": same, "edges": total}),
            vec!["index/mod/__init__, entries and barrels excluded"],
            Vec::new(),
        );
    }
}

/// Lakos cumulative component dependency, normalized against a balanced
/// binary tree of the same size (NCCD > 1: more coupled than a tree).
fn health(v: &View) -> Value {
    let nodes = v
        .files
        .iter()
        .copied()
        .filter(|f| v.production(*f))
        .collect::<Vec<_>>();
    let n = nodes.len();
    if n == 0 {
        return json!({});
    }
    let allowed = nodes.iter().copied().collect::<BTreeSet<_>>();
    let adj = |id: u32| {
        v.out[id as usize]
            .iter()
            .copied()
            .filter(|x| allowed.contains(x))
            .collect::<Vec<_>>()
    };
    // Condense SCCs (singletons included) and sum reach sizes bottom-up.
    let mut comp_of = BTreeMap::<u32, usize>::new();
    let mut sizes = Vec::new();
    for members in strongly_connected(&nodes, &adj, v.t.nodes.len()) {
        for m in &members {
            comp_of.insert(*m, sizes.len());
        }
        sizes.push(members.len());
    }
    for node in &nodes {
        if !comp_of.contains_key(node) {
            comp_of.insert(*node, sizes.len());
            sizes.push(1);
        }
    }
    let k = sizes.len();
    let words = k.div_ceil(64);
    let mut dag = vec![BTreeSet::new(); k];
    for node in &nodes {
        for next in adj(*node) {
            let (a, b) = (comp_of[node], comp_of[&next]);
            if a != b {
                dag[a].insert(b);
            }
        }
    }
    // Reverse topological order via DFS post-order.
    let mut order = Vec::with_capacity(k);
    let mut state = vec![0u8; k];
    for start in 0..k {
        if state[start] != 0 {
            continue;
        }
        let mut stack = vec![(
            start,
            dag[start].iter().copied().collect::<Vec<_>>(),
            0usize,
        )];
        state[start] = 1;
        while let Some((id, next, at)) = stack.last_mut() {
            if let Some(&w) = next.get(*at) {
                *at += 1;
                if state[w] == 0 {
                    state[w] = 1;
                    let succ = dag[w].iter().copied().collect::<Vec<_>>();
                    stack.push((w, succ, 0));
                }
                continue;
            }
            order.push(*id);
            stack.pop();
        }
    }
    let mut reach = vec![vec![0u64; words]; k];
    let mut ccd = 0u64;
    for id in order {
        let mut bits = vec![0u64; words];
        bits[id / 64] |= 1 << (id % 64);
        for next in &dag[id] {
            for (w, word) in bits.iter_mut().enumerate() {
                *word |= reach[*next][w];
            }
        }
        let mut reached = 0u64;
        for (w, word) in bits.iter().enumerate() {
            let mut rest = *word;
            while rest != 0 {
                let bit = rest.trailing_zeros() as usize;
                reached += sizes[w * 64 + bit] as u64;
                rest &= rest - 1;
            }
        }
        ccd += reached * sizes[id] as u64;
        reach[id] = bits;
    }
    let nf = n as f64;
    let balanced = (nf + 1.0) * (nf + 1.0).log2() - nf;
    let tangled = sizes.iter().filter(|s| **s > 1).copied().sum::<usize>();
    json!({
        "files": n,
        "ccd": ccd,
        "nccd": if balanced > 0.0 { ((ccd as f64 / balanced) * 1000.0).round() / 1000.0 } else { 0.0 },
        "averageReach": ((ccd as f64 / nf) * 10.0).round() / 10.0,
        "filesInCycles": tangled,
    })
}

/// Runs the selected detectors (all when `only` is empty), dedupes by
/// subject, and ranks by score.
/// Directories never scanned for text mentions (build output, deps, VCS).
const MENTION_SKIP_DIRS: &[&str] = &[
    "node_modules",
    "dist",
    "build",
    "out",
    "coverage",
    "target",
    ".next",
    ".cache",
    ".git",
    "venv",
    "__pycache__",
    "vendor",
];
const MENTION_MAX_FILE: u64 = 2 << 20;
const MENTION_MAX_TOTAL: u64 = 512 << 20;

type Mentions = BTreeMap<String, Vec<String>>;

/// One pass over the repository's text: which candidate file stems appear as
/// path-like tokens (`'./worker.ts'`, `"src/tool.ts"`) and which export names
/// appear as identifiers, and where (up to 3 files each). This is the
/// SCARF/vulture "is it mentioned anywhere" safety check.
fn scan_mentions(
    root: &std::path::Path,
    stems: &BTreeSet<String>,
    names: &BTreeSet<String>,
) -> (Mentions, Mentions, bool) {
    let mut stem_hits = Mentions::new();
    let mut name_hits = Mentions::new();
    let mut budget = MENTION_MAX_TOTAL;
    let mut truncated = false;
    let walker = ignore::WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .parents(true)
        .filter_entry(|entry| {
            !entry.file_type().is_some_and(|kind| kind.is_dir())
                || !MENTION_SKIP_DIRS.contains(&entry.file_name().to_string_lossy().as_ref())
        })
        .build();
    let record = |map: &mut Mentions, key: &str, file: &str| {
        let files = map.entry(key.to_owned()).or_default();
        if files.len() < 3 && !files.iter().any(|f| f == file) {
            files.push(file.to_owned());
        }
    };
    for entry in walker.flatten() {
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let size = entry.metadata().map_or(0, |meta| meta.len());
        if size > MENTION_MAX_FILE {
            continue;
        }
        if size > budget {
            truncated = true;
            break;
        }
        budget -= size;
        let Ok(bytes) = std::fs::read(entry.path()) else {
            continue;
        };
        if bytes.iter().take(8192).any(|byte| *byte == 0) {
            continue;
        }
        let rel = entry.path().strip_prefix(root).map_or_else(
            |_| entry.path().to_string_lossy().into_owned(),
            |path| path.to_string_lossy().replace('\\', "/"),
        );
        let text = String::from_utf8_lossy(&bytes);
        for token in text.split(|c: char| !(c.is_alphanumeric() || "_$./@-".contains(c))) {
            if token.is_empty() {
                continue;
            }
            if token.contains(['/', '.']) {
                let last = token.rsplit('/').next().unwrap_or(token);
                let stem = last.split('.').next().unwrap_or(last);
                if stems.contains(stem) {
                    record(&mut stem_hits, stem, &rel);
                }
            }
            for part in token.split(['.', '/', '-', '@']) {
                if names.contains(part) {
                    record(&mut name_hits, part, &rel);
                }
            }
        }
    }
    (stem_hits, name_hits, truncated)
}

fn file_stem(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.split('.').next().unwrap_or(name).to_owned()
}

/// Re-tiers dead-code findings with the mention scan.
fn tier_dead_code(findings: &mut [Finding], root: &std::path::Path, notes: &mut Vec<Value>) {
    let mut stems = BTreeSet::new();
    let mut names = BTreeSet::new();
    for finding in findings.iter() {
        match finding.detector {
            "unreachable-file" => {
                stems.insert(file_stem(&finding.subject));
            }
            "unused-export" => {
                for name in finding.evidence["exports"].as_array().into_iter().flatten() {
                    if let Some(name) = name.as_str().filter(|n| n.len() >= 4) {
                        names.insert(name.to_owned());
                    }
                }
            }
            _ => {}
        }
    }
    if stems.is_empty() && names.is_empty() {
        return;
    }
    let (stem_hits, name_hits, truncated) = scan_mentions(root, &stems, &names);
    if truncated {
        notes.push(json!({"detector": "unreachable-file", "skipped": "mention scan stopped at 512 MB of text; later files unchecked"}));
    }
    for finding in findings.iter_mut() {
        match finding.detector {
            "unreachable-file" => {
                let mentions = stem_hits
                    .get(&file_stem(&finding.subject))
                    .map(|files| {
                        files
                            .iter()
                            .filter(|f| **f != finding.subject)
                            .cloned()
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if !mentions.is_empty() {
                    finding.tier = 60;
                    finding.evidence["mentionedIn"] = json!(mentions);
                    finding.controls.push(
                        "its path is mentioned in text (string, config, or script): likely loaded dynamically",
                    );
                } else if finding.evidence["orphan"] == true {
                    finding.tier = 100;
                }
            }
            "unused-export" => {
                let exports = finding.evidence["exports"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let mut mentioned = BTreeMap::new();
                for name in exports.iter().filter_map(Value::as_str) {
                    if let Some(files) = name_hits.get(name) {
                        let others = files
                            .iter()
                            .filter(|f| **f != finding.subject)
                            .cloned()
                            .collect::<Vec<_>>();
                        if !others.is_empty() {
                            mentioned.insert(name.to_owned(), others);
                        }
                    }
                }
                if !mentioned.is_empty() {
                    finding.evidence["mentionedIn"] = json!(mentioned);
                    if mentioned.len() == exports.len() {
                        finding.tier = 60;
                    }
                }
            }
            _ => {}
        }
    }
}

/// Runs the selected detectors (all when `only` is empty) and ranks them.
/// With `root`, dead-code findings are re-tiered by a repo-wide mention scan.
pub(crate) fn run(
    t: &GraphTables,
    only: &[String],
    root: Option<&std::path::Path>,
) -> Result<Report, String> {
    for name in only {
        if !DETECTORS.contains(&name.as_str()) {
            return Err(format!(
                "unknown detector {name:?}; expected one of: {}",
                DETECTORS.join(", ")
            ));
        }
    }
    let wants = |name: &str| only.is_empty() || only.iter().any(|o| o == name);
    let mut c = Ctx {
        v: View::new(t),
        findings: Vec::new(),
        notes: Vec::new(),
    };
    let mut timings = BTreeMap::<&str, u64>::new();
    let steps: [DetectorStep; 12] = [
        ("cycle", &["cycle"], detect_cycles),
        ("dir-cycle", &["dir-cycle"], detect_dir_cycles),
        (
            "reachability",
            &["unreachable-file", "test-only"],
            detect_reachability,
        ),
        ("unused-export", &["unused-export"], detect_unused_exports),
        (
            "dependencies",
            &["undeclared-dependency", "dev-dependency-in-production"],
            detect_dependencies,
        ),
        (
            "boundary-violation",
            &["boundary-violation"],
            detect_boundaries,
        ),
        (
            "unresolved-import",
            &["unresolved-import"],
            detect_unresolved,
        ),
        ("god-file", &["god-file"], detect_hubs),
        ("critical-file", &["critical-file"], detect_critical),
        (
            "single-point-of-failure",
            &["single-point-of-failure"],
            detect_spof,
        ),
        (
            "components",
            &["unstable-dependency", "main-sequence"],
            detect_components,
        ),
        ("misplaced-file", &["misplaced-file"], detect_misplaced),
    ];
    for (label, detectors, step) in steps {
        if detectors.iter().any(|d| wants(d)) {
            let started = std::time::Instant::now();
            step(&mut c);
            timings.insert(label, started.elapsed().as_millis() as u64);
        }
    }
    c.findings.retain(|f| wants(f.detector));
    if let Some(root) = root {
        let started = std::time::Instant::now();
        tier_dead_code(&mut c.findings, root, &mut c.notes);
        timings.insert("mention-scan", started.elapsed().as_millis() as u64);
    }

    // Dedupe by subject: the strongest finding leads, others corroborate.
    let mut by_detector = BTreeMap::<&str, usize>::new();
    for finding in &c.findings {
        *by_detector.entry(finding.detector).or_default() += 1;
    }
    c.findings
        .sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
    let mut merged: Vec<Finding> = Vec::new();
    let mut lead = BTreeMap::<String, usize>::new();
    for finding in c.findings {
        match lead.get(&finding.subject) {
            Some(&at) => {
                let leader = &mut merged[at];
                if leader.detector != finding.detector
                    && !leader.corroborated_by.contains(&finding.detector)
                {
                    leader.corroborated_by.push(finding.detector);
                    leader.score = (leader.score * 1.1).min(1.0);
                }
            }
            None => {
                lead.insert(finding.subject.clone(), merged.len());
            }
        }
        merged.push(finding);
    }
    merged.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
    let entries =
        c.v.entries
            .values()
            .fold(BTreeMap::<&str, usize>::new(), |mut acc, rule| {
                *acc.entry(rule).or_default() += 1;
                acc
            });
    let mut roles = BTreeMap::<&str, usize>::new();
    for file in &c.v.files {
        for role in role_names(c.v.flags(*file)) {
            *roles.entry(role).or_default() += 1;
        }
    }
    let summary = json!({
        "findings": merged.len(),
        "byDetector": by_detector,
        "entrypoints": {"total": c.v.entries.len(), "byRule": entries},
        "fileRoles": roles,
        "health": health(&c.v),
        "detectorMs": timings,
        "notes": c.notes,
        "method": "hypotheses ranked by severity × confidence × (0.5 + 0.5 × PageRank percentile); verify before acting",
    });
    Ok(Report {
        findings: merged,
        summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feedback_arcs_break_every_cycle() {
        // 0→1→2→0 and 1→0: removing the returned arcs must leave a DAG.
        let adj = |id: u32| match id {
            0 => vec![1],
            1 => vec![2, 0],
            2 => vec![0],
            _ => vec![],
        };
        let cuts = feedback_arcs(&[0, 1, 2], &adj);
        assert!(!cuts.is_empty() && cuts.len() <= 2, "{cuts:?}");
        let pruned = |id: u32| {
            adj(id)
                .into_iter()
                .filter(|to| !cuts.contains(&(id, *to)))
                .collect::<Vec<_>>()
        };
        assert!(strongly_connected(&[0, 1, 2], &pruned, 3).is_empty());
    }

    #[test]
    fn witness_is_the_shortest_cycle_through_the_start() {
        let adj = |id: u32| match id {
            0 => vec![1, 3],
            1 => vec![2],
            2 => vec![0],
            3 => vec![0],
            _ => vec![],
        };
        let members = BTreeSet::from([0, 1, 2, 3]);
        assert_eq!(witness_cycle(0, &members, &adj), vec![0, 3, 0]);
    }

    #[test]
    fn magnitude_is_bounded() {
        assert!((magnitude(0) - 0.6).abs() < 1e-9);
        assert!(magnitude(1_000_000) <= 1.0);
    }
}
