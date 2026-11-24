// What changed structurally between two saved graphs (two runs, or a run and the graph its last refresh
// replaced): files added, removed and renamed; production dependencies added, removed and re-weighted; cycles that
// appeared, grew, shrank or resolved; new dependencies between chunks (packages) and which of them point from a
// more stable package to a less stable one; how many files now depend on each touched file; files that entered or
// left the top of the rankings. Every list is complete; `review` orders the findings a reviewer reads first.
// Pure functions over data.json, like the query engine.
import { csr, type FileEdge } from '../graph-model.ts';
import type { QueryData } from './query-index.ts';
import type {
  CycleSet,
  Link,
  PackageCoupling,
  UnresolvedLink,
} from './types.ts';
import { plural } from './query-ops.ts';

/** Ranking positions compared between the graphs: a file entering or leaving this head is reported. */
export const RANK_HEAD = 20;

export interface Rename {
  from: string;
  to: string;
  /** `extension`: same path, new extension; `move`: same file name, the only one with it on both sides. */
  rule: 'extension' | 'move';
}

export interface EdgeChange {
  source: string;
  target: string;
  before: number;
  after: number;
  crossChunk: boolean;
}

export interface CycleChange {
  change: 'new' | 'grown' | 'shrunk' | 'reshaped' | 'resolved';
  before: string[];
  after: string[];
  added: string[];
  removed: string[];
}

export interface PackageLink {
  change: 'added' | 'removed';
  from: string;
  to: string;
  edges: number;
  /** Added only: the depending chunk is more stable (lower instability) than the chunk it now depends on. */
  againstStability?: boolean;
  instability?: { from: number | null; to: number | null };
}

export interface CouplingChange {
  chunk: string;
  before: Pick<PackageCoupling, 'ca' | 'ce' | 'instability'> | null;
  after: Pick<PackageCoupling, 'ca' | 'ce' | 'instability'> | null;
}

export interface ReachChange {
  file: string;
  before: number;
  after: number;
}

export interface RankChange {
  metric: 'pagerank' | 'betweenness';
  file: string;
  change: 'entered' | 'left';
  before: number | null;
  after: number | null;
}

export interface Finding {
  kind: 'cycle' | 'package-link' | 'reach' | 'rank' | 'doc-link';
  severity: 'high' | 'medium' | 'low';
  text: string;
  files: string[];
}

export interface GraphDiff {
  summary: Record<string, number>;
  review: Finding[];
  files: { added: string[]; removed: string[]; renamed: Rename[] };
  edges: {
    added: EdgeChange[];
    removed: EdgeChange[];
    reweighted: EdgeChange[];
  };
  calls: { added: number; removed: number };
  cycles: CycleChange[];
  packages: { links: PackageLink[]; coupling: CouplingChange[] };
  reach: ReachChange[];
  ranks: RankChange[];
  /** Doc references that broke and citations that went stale since `before` (both graphs read their docs). */
  links: { broken: UnresolvedLink[]; stale: Link[] };
}

const BROKEN = new Set(['no-such-file', 'no-such-anchor', 'line-out-of-range']);
const referenceKey = (u: UnresolvedLink) =>
  `${u.source}\0${u.line}\0${u.text}\0${u.reason}`;
const citationKey = (l: Link) =>
  `${l.source}\0${l.line}\0${l.target}\0${l.lines?.join('-')}`;

// Only graphs that both read their docs compare: an older graph without links would make every reference "new".
function linkChanges(before: QueryData, after: QueryData) {
  if (!before.links || !after.links) return { broken: [], stale: [] };
  const was = new Set((before.unresolvedLinks ?? []).map(referenceKey));
  const wasStale = new Set(
    before.links.filter(l => l.check === 'stale').map(citationKey)
  );
  return {
    broken: (after.unresolvedLinks ?? []).filter(
      u => BROKEN.has(u.reason) && !was.has(referenceKey(u))
    ),
    stale: after.links.filter(
      l => l.check === 'stale' && !wasStale.has(citationKey(l))
    ),
  };
}

const ext = /\.[^./]+$/;
const base = (id: string) => id.slice(id.lastIndexOf('/') + 1);
const stem = (id: string) => id.replace(ext, '');
const sources = (D: QueryData) =>
  (D.files ?? []).filter(f => f.kind !== 'package').map(f => f.id);

// A removed file and an added one are the same file when they share a path without its extension (a .mjs
// that became .ts), or when both are the only file with their name on each side (a move). Anything else is a
// removal and an addition: a guess would hide a real change.
export function renames(removed: string[], added: string[]): Rename[] {
  const out: Rename[] = [];
  const taken = new Set<string>();
  const byStem = Map.groupBy(added, stem);
  for (const from of removed) {
    const same = byStem.get(stem(from)) ?? [];
    if (same.length === 1) {
      out.push({ from, to: same[0], rule: 'extension' });
      taken.add(from).add(same[0]);
    }
  }
  const restRemoved = removed.filter(id => !taken.has(id));
  const restAdded = added.filter(id => !taken.has(id));
  const removedByName = Map.groupBy(restRemoved, base);
  const addedByName = Map.groupBy(restAdded, base);
  for (const [name, froms] of removedByName) {
    const tos = addedByName.get(name) ?? [];
    if (froms.length === 1 && tos.length === 1)
      out.push({ from: froms[0], to: tos[0], rule: 'move' });
  }
  return out.toSorted((a, b) => a.to.localeCompare(b.to));
}

// Production file edges (the weight rule of graph-model.ts), with ids translated into the newer graph's names.
function productionEdges(D: QueryData, name: (id: string) => string) {
  const edges = new Map<string, FileEdge>();
  for (const e of D.fileEdges ?? []) {
    if (e.test) continue;
    const source = name(e.source),
      target = name(e.target);
    edges.set(`${source}\0${target}`, {
      ...e,
      source,
      target,
      strong: e.strong !== false,
      load: e.load ?? null,
    });
  }
  return edges;
}

function chunkOf(D: QueryData) {
  const chunk = new Map((D.files ?? []).map(f => [f.id, f.chunk ?? '-']));
  return (id: string) => chunk.get(id) ?? '-';
}

function edgeChanges(
  before: Map<string, FileEdge>,
  after: Map<string, FileEdge>,
  chunkBefore: (id: string) => string,
  chunkAfter: (id: string) => string
) {
  const row = (
    e: FileEdge,
    b: number,
    a: number,
    chunk: (id: string) => string
  ): EdgeChange => ({
    source: e.source,
    target: e.target,
    before: b,
    after: a,
    crossChunk: chunk(e.source) !== chunk(e.target),
  });
  const added: EdgeChange[] = [],
    removed: EdgeChange[] = [],
    reweighted: EdgeChange[] = [];
  for (const [key, e] of after) {
    const was = before.get(key);
    if (!was) added.push(row(e, 0, e.weight, chunkAfter));
    else if (was.weight !== e.weight)
      reweighted.push(row(e, was.weight, e.weight, chunkAfter));
  }
  for (const [key, e] of before)
    if (!after.has(key)) removed.push(row(e, e.weight, 0, chunkBefore));
  const byWeight = (a: EdgeChange, b: EdgeChange) =>
    Math.abs(b.after - b.before) - Math.abs(a.after - a.before) ||
    a.source.localeCompare(b.source) ||
    a.target.localeCompare(b.target);
  return {
    added: added.sort(byWeight),
    removed: removed.sort(byWeight),
    reweighted: reweighted.sort(byWeight),
  };
}

// Each cycle of the newer graph against the older cycle it overlaps most; an older cycle no newer one overlaps is
// resolved. Cycles come from the saved metrics (strongly connected sets of the production file graph).
function cycleChanges(
  before: CycleSet[],
  after: CycleSet[],
  name: (id: string) => string
): CycleChange[] {
  const old = before.map(c => new Set(c.members.map(name)));
  const matched = new Set<number>();
  const out: CycleChange[] = [];
  for (const c of after) {
    const members = new Set(c.members);
    let best = -1,
      overlap = 0;
    old.forEach((set, i) => {
      let n = 0;
      for (const id of set) if (members.has(id)) n++;
      if (n > overlap) [best, overlap] = [i, n];
    });
    const was = best >= 0 ? old[best] : new Set<string>();
    if (best >= 0) matched.add(best);
    const added = c.members.filter(id => !was.has(id)).sort();
    const removed = [...was].filter(id => !members.has(id)).sort();
    if (!added.length && !removed.length) continue;
    const change =
      best < 0
        ? 'new'
        : !removed.length
          ? 'grown'
          : !added.length
            ? 'shrunk'
            : 'reshaped';
    out.push({
      change,
      before: [...was].sort(),
      after: [...c.members].sort(),
      added,
      removed,
    });
  }
  old.forEach((set, i) => {
    if (!matched.has(i))
      out.push({
        change: 'resolved',
        before: [...set].sort(),
        after: [],
        added: [],
        removed: [...set].sort(),
      });
  });
  const order = { new: 0, grown: 1, reshaped: 2, shrunk: 3, resolved: 4 };
  return out.sort(
    (a, b) =>
      order[a.change] - order[b.change] ||
      b.after.length - a.after.length ||
      b.before.length - a.before.length
  );
}

// Chunk-to-chunk dependencies (a production file edge whose ends lie in different chunks), added or removed.
function packageLinks(
  before: Map<string, FileEdge>,
  after: Map<string, FileEdge>,
  chunkBefore: (id: string) => string,
  chunkAfter: (id: string) => string,
  instability: Map<string, number | null>
): PackageLink[] {
  const links = (
    edges: Map<string, FileEdge>,
    chunk: (id: string) => string
  ) => {
    const by = new Map<string, number>();
    for (const e of edges.values()) {
      const [a, b] = [chunk(e.source), chunk(e.target)];
      if (a !== b) by.set(`${a}\0${b}`, (by.get(`${a}\0${b}`) ?? 0) + 1);
    }
    return by;
  };
  const was = links(before, chunkBefore),
    now = links(after, chunkAfter);
  const out: PackageLink[] = [];
  for (const [key, edges] of now) {
    if (was.has(key)) continue;
    const [from, to] = key.split('\0');
    const i = {
      from: instability.get(from) ?? null,
      to: instability.get(to) ?? null,
    };
    out.push({
      change: 'added',
      from,
      to,
      edges,
      instability: i,
      againstStability: i.from != null && i.to != null && i.from < i.to,
    });
  }
  for (const [key, edges] of was)
    if (!now.has(key)) {
      const [from, to] = key.split('\0');
      out.push({ change: 'removed', from, to, edges });
    }
  return out.sort(
    (a, b) =>
      (a.change === 'added' ? 0 : 1) - (b.change === 'added' ? 0 : 1) ||
      Number(b.againstStability ?? false) -
        Number(a.againstStability ?? false) ||
      b.edges - a.edges
  );
}

function couplingChanges(
  before: QueryData,
  after: QueryData
): CouplingChange[] {
  const pick = (p: PackageCoupling) => ({
    ca: p.ca,
    ce: p.ce,
    instability: p.instability,
  });
  const was = new Map(
    (before.metrics?.packages ?? []).map(p => [p.chunk, pick(p)])
  );
  const now = new Map(
    (after.metrics?.packages ?? []).map(p => [p.chunk, pick(p)])
  );
  const out: CouplingChange[] = [];
  for (const chunk of new Set([...was.keys(), ...now.keys()])) {
    const b = was.get(chunk) ?? null,
      a = now.get(chunk) ?? null;
    if (JSON.stringify(b) !== JSON.stringify(a))
      out.push({ chunk, before: b, after: a });
  }
  return out.sort(
    (x, y) =>
      (y.after?.ca ?? 0) +
      (y.after?.ce ?? 0) -
      ((x.after?.ca ?? 0) + (x.after?.ce ?? 0))
  );
}

// How many files depend on each file, transitively (reverse reachability over production edges).
function dependents(ids: string[], edges: Map<string, FileEdge>, of: string[]) {
  const G = csr(ids, [...edges.values()]);
  const out = new Map<string, number>();
  for (const id of of) {
    const start = G.index.get(id);
    if (start === undefined) continue;
    const seen = new Uint8Array(G.n);
    seen[start] = 1;
    const queue = [start];
    for (let head = 0; head < queue.length; head++) {
      const u = queue[head];
      for (let k = G.in.offsets[u]; k < G.in.offsets[u + 1]; k++) {
        const v = G.in.targets[k];
        if (!seen[v]) {
          seen[v] = 1;
          queue.push(v);
        }
      }
    }
    out.set(id, queue.length - 1);
  }
  return out;
}

function rankChanges(
  before: QueryData,
  after: QueryData,
  name: (id: string) => string
): RankChange[] {
  const out: RankChange[] = [];
  for (const metric of ['pagerank', 'betweenness'] as const) {
    const head = (D: QueryData, rename: (id: string) => string) =>
      new Map(
        (D.files ?? [])
          .filter(f => typeof f.metrics?.[metric] === 'number')
          .toSorted(
            (a, b) =>
              (b.metrics?.[metric] ?? 0) - (a.metrics?.[metric] ?? 0) ||
              a.id.localeCompare(b.id)
          )
          .slice(0, RANK_HEAD)
          .map((f, i) => [rename(f.id), i + 1])
      );
    const was = head(before, name),
      now = head(after, id => id);
    for (const [file, rank] of now)
      if (!was.has(file))
        out.push({
          metric,
          file,
          change: 'entered',
          before: null,
          after: rank,
        });
    for (const [file, rank] of was)
      if (!now.has(file))
        out.push({ metric, file, change: 'left', before: rank, after: null });
  }
  return out;
}

function review(d: Omit<GraphDiff, 'review' | 'summary'>): Finding[] {
  const out: Finding[] = [];
  for (const c of d.cycles) {
    if (c.change === 'new')
      out.push({
        kind: 'cycle',
        severity: 'high',
        text: `new dependency cycle of ${plural(c.after.length, 'file')}`,
        files: c.after,
      });
    else if (c.change === 'grown' || c.change === 'reshaped')
      out.push({
        kind: 'cycle',
        severity: 'high',
        text: `cycle ${c.change} from ${c.before.length} to ${plural(c.after.length, 'file')}: +${c.added.length}${c.removed.length ? ` −${c.removed.length}` : ''}`,
        files: c.added,
      });
    else
      out.push({
        kind: 'cycle',
        severity: 'low',
        text:
          c.change === 'resolved'
            ? `cycle of ${plural(c.before.length, 'file')} resolved`
            : `cycle shrank from ${c.before.length} to ${plural(c.after.length, 'file')}`,
        files: c.removed,
      });
  }
  for (const l of d.packages.links.filter(x => x.change === 'added'))
    out.push({
      kind: 'package-link',
      severity: l.againstStability ? 'high' : 'medium',
      text: `new dependency ${l.from} → ${l.to} (${plural(l.edges, 'file edge')})${l.againstStability ? `: a more stable package (instability ${l.instability?.from}) now depends on a less stable one (${l.instability?.to})` : ''}`,
      files: d.edges.added.filter(e => e.crossChunk).map(e => e.source),
    });
  for (const r of d.reach.filter(x => x.after > x.before))
    out.push({
      kind: 'reach',
      severity: 'medium',
      text: `${r.file}: ${plural(r.after, 'file')} depend${r.after === 1 ? 's' : ''} on it now (was ${r.before})`,
      files: [r.file],
    });
  for (const r of d.ranks.filter(x => x.change === 'entered'))
    out.push({
      kind: 'rank',
      severity: 'low',
      text: `${r.file} entered the ${r.metric} top ${RANK_HEAD} at #${r.after}`,
      files: [r.file],
    });
  const level = { high: 0, medium: 1, low: 2 };
  return out.sort((a, b) => level[a.severity] - level[b.severity]);
}

/** The structural change from `before` to `after` (two saved graphs of the same project). */
export function diffGraphs(before: QueryData, after: QueryData): GraphDiff {
  const was = sources(before),
    now = sources(after);
  const nowSet = new Set(now),
    wasSet = new Set(was);
  const gone = was.filter(id => !nowSet.has(id)),
    fresh = now.filter(id => !wasSet.has(id));
  const renamed = renames(gone, fresh);
  const to = new Map(renamed.map(r => [r.from, r.to]));
  const name = (id: string) => to.get(id) ?? id;
  const movedFrom = new Set(renamed.map(r => r.from)),
    movedTo = new Set(renamed.map(r => r.to));
  const files = {
    added: fresh.filter(id => !movedTo.has(id)).sort(),
    removed: gone.filter(id => !movedFrom.has(id)).sort(),
    renamed,
  };
  const edgesBefore = productionEdges(before, name),
    edgesAfter = productionEdges(after, id => id);
  const chunkBefore = chunkOf(before),
    chunkAfter = chunkOf(after);
  // Older edges carry newer names; their chunk is the one the older graph recorded for the old name.
  const back = new Map(renamed.map(r => [r.to, r.from]));
  const beforeChunkRenamed = (id: string) => chunkBefore(back.get(id) ?? id);
  const edges = edgeChanges(
    edgesBefore,
    edgesAfter,
    beforeChunkRenamed,
    chunkAfter
  );
  const callKey = (
    c: { source: string; target: string },
    rename: (id: string) => string
  ) => {
    const [sf, ss = ''] = c.source.split('#');
    const [tf, ts = ''] = c.target.split('#');
    return `${rename(sf)}#${ss}\0${rename(tf)}#${ts}`;
  };
  const callsBefore = new Set(
    (before.calls ?? []).filter(c => !c.test).map(c => callKey(c, name))
  );
  const callsAfter = new Set(
    (after.calls ?? []).filter(c => !c.test).map(c => callKey(c, id => id))
  );
  const calls = {
    added: [...callsAfter].filter(k => !callsBefore.has(k)).length,
    removed: [...callsBefore].filter(k => !callsAfter.has(k)).length,
  };
  const cycles = cycleChanges(
    before.metrics?.cycles ?? [],
    after.metrics?.cycles ?? [],
    name
  );
  const instability = new Map(
    (after.metrics?.packages ?? []).map(p => [p.chunk, p.instability])
  );
  const packages = {
    links: packageLinks(
      edgesBefore,
      edgesAfter,
      beforeChunkRenamed,
      chunkAfter,
      instability
    ),
    coupling: couplingChanges(before, after),
  };
  // Files an edge change or a new file touches: how many files depend on each, before and after.
  const touched = [
    ...new Set([
      ...[...edges.added, ...edges.removed, ...edges.reweighted].map(
        e => e.target
      ),
      ...files.added,
      ...renamed.map(r => r.to),
    ]),
  ];
  const beforeIds = was.map(name);
  const reachBefore = dependents(beforeIds, edgesBefore, touched);
  const reachAfter = dependents(now, edgesAfter, touched);
  const reach = touched
    .map(file => ({
      file,
      before: reachBefore.get(file) ?? 0,
      after: reachAfter.get(file) ?? 0,
    }))
    .filter(r => r.before !== r.after)
    .sort(
      (a, b) =>
        b.after - b.before - (a.after - a.before) ||
        a.file.localeCompare(b.file)
    );
  const ranks = rankChanges(before, after, name);
  const links = linkChanges(before, after);
  const parts = { files, edges, calls, cycles, packages, reach, ranks, links };
  const findings = review(parts);
  if (links.broken.length)
    findings.push({
      kind: 'doc-link',
      severity: 'medium',
      text: `${links.broken.length} doc references broke: ${links.broken.map(u => `${u.source}:${u.line} ${u.text} (${u.reason})`).join('; ')}`,
      files: [...new Set(links.broken.map(u => u.source))],
    });
  if (links.stale.length)
    findings.push({
      kind: 'doc-link',
      severity: 'medium',
      text: `${links.stale.length} doc citations went stale (the cited code changed): ${links.stale.map(l => `${l.source}:${l.line} → ${l.target}:${l.lines?.join('-')}`).join('; ')}`,
      files: [...new Set(links.stale.map(l => l.source))],
    });
  return {
    summary: {
      filesAdded: files.added.length,
      filesRemoved: files.removed.length,
      filesRenamed: renamed.length,
      edgesAdded: edges.added.length,
      edgesRemoved: edges.removed.length,
      edgesReweighted: edges.reweighted.length,
      crossChunkEdgesAdded: edges.added.filter(e => e.crossChunk).length,
      callsAdded: calls.added,
      callsRemoved: calls.removed,
      cyclesNew: cycles.filter(c => c.change === 'new').length,
      cyclesGrown: cycles.filter(
        c => c.change === 'grown' || c.change === 'reshaped'
      ).length,
      cyclesResolved: cycles.filter(c => c.change === 'resolved').length,
      packageLinksAdded: packages.links.filter(l => l.change === 'added')
        .length,
      againstStability: packages.links.filter(l => l.againstStability).length,
      reachGrew: reach.filter(r => r.after > r.before).length,
      findingsHigh: findings.filter(f => f.severity === 'high').length,
      docReferencesBroken: links.broken.length,
      docCitationsStale: links.stale.length,
    },
    review: findings,
    ...parts,
  };
}

/** Plain text: the summary, then every review finding (high first); `--json` carries every row of every list. */
export function formatDiff(d: GraphDiff) {
  const s = d.summary;
  const lines = [
    `files +${s.filesAdded} −${s.filesRemoved} renamed ${s.filesRenamed} · dependencies +${s.edgesAdded} −${s.edgesRemoved} reweighted ${s.edgesReweighted} (${s.crossChunkEdgesAdded} new between chunks) · calls +${s.callsAdded} −${s.callsRemoved}`,
    `cycles new ${s.cyclesNew}, grown ${s.cyclesGrown}, resolved ${s.cyclesResolved} · package links +${s.packageLinksAdded} (${s.againstStability} against stability) · ${s.reachGrew} files with more dependents`,
  ];
  if (!d.review.length)
    lines.push(
      'Nothing to review: no cycle, package link, reach, ranking or doc reference changed.'
    );
  else lines.push(`Review (${d.review.length}, high first):`);
  for (const f of d.review)
    lines.push(
      `  [${f.severity}] ${f.text}${f.kind === 'cycle' && f.files.length ? `: ${f.files.join(', ')}` : ''}`
    );
  for (const [title, list] of [
    ['dependencies added', d.edges.added],
    ['dependencies removed', d.edges.removed],
  ] as const)
    if (list.length)
      lines.push(
        `${title} (${list.length}):`,
        ...list.map(
          e =>
            `  ${e.source} → ${e.target}  w ${e.before} → ${e.after}${e.crossChunk ? '  between chunks' : ''}`
        )
      );
  if (d.files.renamed.length)
    lines.push(
      `renamed (${d.files.renamed.length}):`,
      ...d.files.renamed.map(r => `  ${r.from} → ${r.to}  (${r.rule})`)
    );
  for (const [title, list] of [
    ['files added', d.files.added],
    ['files removed', d.files.removed],
  ] as const)
    if (list.length)
      lines.push(`${title} (${list.length}):`, ...list.map(id => `  ${id}`));
  return lines.join('\n');
}
