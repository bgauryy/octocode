// reach <entry...> [--eager] [--forbid <file|dir|pkg:name>...] [depth N]
// What an entry point loads at run time: every file and package its imports pull in (type-only imports never run),
// split into what loads with the entry (eager) and what loads only behind a dynamic import() (lazy), with bytes, the
// heaviest packages and the shortest chain that pulls each package in; each file row carries its `chain` from the
// entry. `--eager` answers startup alone. `--forbid` states a rule ("the worker must not load pkg:@scope/ai"): every
// forbidden file or package reached is a violation with its chain and the answer is `failed`, so
// `octocode-graph query` exits 1 in CI.
// why <entry> <file|pkg:name> [--eager]: why the entry loads the target: the route through every distinct first hop
// of the entry that still reaches it at run time (path --all over the runtime walk), each hop with its import line
// and kinds.
import {
  QueryError,
  edgeOf,
  evidenceLine,
  evidenceOf,
  importLine,
  indexOf,
  resolveFile,
  type QueryIndex,
} from './query-index.ts';
import {
  fileRow,
  firstHopRoutes,
  plural,
  walk,
  type OpResult,
  type Pair,
  type QueryArgs,
  type Row,
} from './query-ops.ts';
import { bytesOf, formatBytes, packageOf } from './query-group.ts';
import { keeps, loadOf, type Load, type LoadMode } from '../loads.ts';
import { nearest } from './query-forgiving.ts';
import {
  packageBytes,
  type BytesEvidence,
  type BytesMethod,
} from './package-bytes.ts';
import type { ExternalPackage } from './types.ts';

/** How a reached file or package loads from the entry: with it, or only behind a dynamic import(). */
type Reached = 'eager' | 'lazy';

interface PackageRow {
  id: string;
  /** `source`: files of this graph grouped by workspace package; `workspace`: a workspace package the graph links
   * as one node; `external`: an installed package; `builtin`: the runtime's own module (node:fs). */
  kind: 'source' | 'workspace' | 'external' | 'builtin';
  load: Reached;
  files?: number;
  bytes: number | null;
  /** A workspace package node: the bytes of every file of its chunk, the most it can add (its entry decides). */
  bytesIfWhole?: number | null;
  /** An installed package the build measured (package-bytes.ts): how, and the evidence; absent when it measured none. */
  bytesMethod?: BytesMethod | null;
  bytesEvidence?: BytesEvidence;
  importers?: string[];
  chain: string[];
}

/** One step of an evidence chain: a file (or a package, last) and, toward the next step, its import's line and kinds. */
export interface Hop {
  id: string;
  nextFile?: string;
  line?: number;
  kinds?: string[];
  /** What the kinds amount to (loads.ts): why the hop counts in a walk of what runs. */
  load?: Load;
}

// An installed package's import by each file, as the build recorded it: the kinds, and the lines where a build
// recorded them.
type PackageImports = Pick<
  ExternalPackage,
  'id' | 'files' | 'testFiles' | 'kinds' | 'lines' | 'loads'
>;
const externalsById = new WeakMap<object, Map<string, PackageImports>>();
function externalOf(R: QueryIndex, pkg: string) {
  let byId = externalsById.get(R.D);
  if (!byId) {
    byId = new Map((R.D.externals ?? []).map(x => [x.id, x]));
    externalsById.set(R.D, byId);
  }
  return byId.get(pkg);
}

/** Every step from the first id to the last, each with the line and kinds of its import of the next step. */
export function chainHops(R: QueryIndex, chain: string[]): Hop[] {
  return chain.map((id, i) => {
    const next = chain[i + 1];
    if (!next) return { id };
    if (next.startsWith('pkg:') && !R.file.has(next)) {
      const x = externalOf(R, next);
      const kinds = x?.kinds?.[id] ?? [];
      const lines = x?.lines?.[id] ?? [];
      return {
        id,
        nextFile: next,
        ...(lines.length && { line: Math.min(...lines) }),
        ...(kinds.length && { kinds, load: loadOf(kinds, x?.loads) }),
      };
    }
    const ev = evidenceOf(R, id, next);
    return {
      id,
      nextFile: next,
      line: evidenceLine(R, id, next),
      kinds: ev.kinds,
      ...(ev.load && { load: ev.load }),
    };
  });
}

/** A chain as one line: each step with its import line and kinds (`a.ts:3 [static-import] → b.ts → pkg:x`). */
export const hopsText = (chain: readonly Hop[]) =>
  chain
    .map(
      h =>
        `${h.id}${h.line ? `:${h.line}` : ''}${h.kinds?.length ? ` [${h.kinds.join(',')}${h.load ? ` → ${h.load}` : ''}]` : ''}`
    )
    .join(' → ');

/** The packages a graph imports: workspace package nodes and installed packages. */
export const knownPackages = (R: QueryIndex) => [
  ...R.ids.filter(id => id.startsWith('pkg:')),
  ...(R.D.externals ?? []).map(x => x.id),
];

/**
 * The files that import an installed package with a load `mode` keeps (null: every load), production files unless
 * the index has tests. A package no file of the graph imports is an error with near names: a misspelled name must
 * never read as "not loaded".
 */
export function packageImporters(
  R: QueryIndex,
  pkg: string,
  mode: LoadMode | null
) {
  const x = externalOf(R, pkg);
  if (!x)
    throw new QueryError(
      `${pkg} names no package this graph imports; check the name`,
      { candidates: nearest(pkg, knownPackages(R)).best.toSorted(), arg: pkg }
    );
  return [...new Set(x.files ?? [])]
    .filter(
      file =>
        R.file.has(file) &&
        (R.tests || !x.testFiles?.includes(file)) &&
        (!mode || keeps(mode, loadOf(x.kinds?.[file], x.loads)))
    )
    .toSorted();
}

interface Violation {
  forbidden: string;
  target: string;
  load: Reached;
  chain: Hop[];
}

const sum = (xs: (number | null)[]) =>
  xs.some(x => x == null) ? null : xs.reduce<number>((a, x) => a + (x ?? 0), 0);

export function reach(I: QueryIndex, q: QueryArgs): OpResult {
  const at = q.args.indexOf('--forbid');
  const entries = at < 0 ? q.args : q.args.slice(0, at);
  const forbid = at < 0 ? [] : q.args.slice(at + 1);
  if (!entries.length)
    throw new QueryError(
      'reach needs an entry file: reach <entry...> [--eager] [--forbid <file|dir|pkg:name>...]'
    );
  if (at >= 0 && !forbid.length)
    throw new QueryError('--forbid needs files, directories or pkg:<name>');
  const eagerOnly = I.load === 'eager';
  const mode = eagerOnly ? 'eager' : 'runtime';
  const R = indexOf(I.D, { tests: I.tests, load: mode });
  const E = eagerOnly ? R : indexOf(I.D, { tests: I.tests, load: 'eager' });
  const seeds = [...new Set(entries.flatMap(arg => entryFiles(R, arg)))];
  const maxDepth = q.depth ?? Infinity;
  const all = walk(R, seeds, { dir: 'out', maxDepth });
  const startup = eagerOnly ? all : walk(E, seeds, { dir: 'out', maxDepth });
  const eager = new Set([...startup.depth.keys()].map(v => E.ids[v]));
  const loadOfFile = (id: string): Reached =>
    eager.has(id) ? 'eager' : 'lazy';

  // The chain that reached a file: its first-found parent at each depth, back to an entry.
  const chainOf = (id: string) => {
    const ids = [id];
    for (let v = R.G.index.get(id); v !== undefined;) {
      const parent = all.via.get(v)?.[0];
      if (parent === undefined) break;
      ids.push(R.ids[parent]);
      v = parent;
    }
    return ids.reverse();
  };

  const rows: Row[] = [];
  const edges: Pair[] = [];
  for (const [v, [u, w]] of all.via) {
    const [id, through] = [R.ids[v], R.ids[u]];
    edges.push([through, id]);
    rows.push(
      fileRow(R, id, {
        depth: all.depth.get(v),
        weight: Number(w.toFixed(3)),
        through,
        load: loadOfFile(id),
        bytes: bytesOf(R, id),
        importedAt: { path: through, line: evidenceLine(R, through, id) },
        kinds: evidenceOf(R, through, id).kinds,
        chain: chainHops(R, chainOf(id)),
      })
    );
  }
  rows.sort(
    (a, b) =>
      (a.depth ?? 0) - (b.depth ?? 0) ||
      Number(b.bytes ?? 0) - Number(a.bytes ?? 0) ||
      (a.id < b.id ? -1 : 1)
  );
  const reached = new Set([...seeds, ...rows.map(r => r.id)]);
  const sourceIds = [...reached].filter(id => !id.startsWith('pkg:'));

  // Packages: this graph's own files by workspace package, workspace packages linked as one node, installed ones.
  const packages: PackageRow[] = [];
  for (const [id, files] of Map.groupBy(sourceIds, f => packageOf(R, f))) {
    const load = files.some(f => eager.has(f)) ? 'eager' : 'lazy';
    const first = files.toSorted(
      (a, b) =>
        (all.depth.get(R.G.index.get(a)!) ?? 0) -
        (all.depth.get(R.G.index.get(b)!) ?? 0)
    )[0];
    packages.push({
      id,
      kind: 'source',
      load,
      files: files.length,
      bytes: sum(files.map(f => bytesOf(R, f))),
      chain: chainOf(first),
    });
  }
  for (const id of reached) {
    if (!id.startsWith('pkg:') || !R.file.has(id)) continue;
    const owner = R.file.get(id)?.chunk;
    const inside = R.ids.filter(
      f => !f.startsWith('pkg:') && R.file.get(f)?.chunk === owner
    );
    packages.push({
      id,
      kind: 'workspace',
      load: loadOfFile(id),
      files: inside.length,
      bytes: null,
      bytesIfWhole: sum(inside.map(f => bytesOf(R, f))),
      chain: chainOf(id),
    });
  }
  const boundaries: { source: string; target: string; line?: number }[] = [];
  for (const x of I.D.externals ?? []) {
    const importers: { file: string; load: Load }[] = [];
    for (const file of new Set(x.files ?? [])) {
      if (!reached.has(file) || (!I.tests && x.testFiles?.includes(file)))
        continue;
      const load = loadOf(x.kinds?.[file], x.loads);
      if (keeps(mode, load)) importers.push({ file, load });
    }
    if (!importers.length) continue;
    const loadsAtStart = importers.some(
      i => eager.has(i.file) && i.load !== 'deferred'
    );
    for (const i of importers)
      if (eager.has(i.file) && i.load === 'deferred' && !loadsAtStart)
        boundaries.push({ source: i.file, target: x.id });
    const closest = importers
      .map(i => i.file)
      .toSorted(
        (a, b) =>
          (all.depth.get(R.G.index.get(a) ?? -1) ?? 0) -
            (all.depth.get(R.G.index.get(b) ?? -1) ?? 0) || (a < b ? -1 : 1)
      )[0];
    const builtin = importers.some(i =>
      x.kinds?.[i.file]?.includes('external-builtin')
    );
    const size = packageBytes(I.D, x.id);
    packages.push({
      id: x.id,
      kind: builtin ? 'builtin' : 'external',
      load: loadsAtStart ? 'eager' : 'lazy',
      bytes: size?.bytes ?? null,
      ...(size?.bytesMethod && {
        bytesMethod: size.bytesMethod,
        bytesEvidence: size.bytesEvidence,
      }),
      importers: importers.map(i => i.file).toSorted(),
      chain: [...chainOf(closest), x.id],
    });
  }
  packages.sort(
    (a, b) =>
      (b.bytes ?? b.bytesIfWhole ?? -1) - (a.bytes ?? a.bytesIfWhole ?? -1) ||
      (b.importers?.length ?? 0) - (a.importers?.length ?? 0) ||
      (a.id < b.id ? -1 : 1)
  );

  // Lazy boundaries: a dynamic import() from a file that loads at startup to one that does not.
  // Unsettled: walked imports whose load the graph could not establish (counted as loading).
  const unsettled: { source: string; target: string; kinds: string[] }[] = [];
  for (const u of reached) {
    const i = R.G.index.get(u);
    if (i === undefined) continue;
    for (let k = R.G.out.offsets[i]; k < R.G.out.offsets[i + 1]; k++) {
      const v = R.ids[R.G.out.targets[k]];
      const load = edgeOf(R, u, v)?.load;
      if (load === 'deferred' && eager.has(u) && !eager.has(v))
        boundaries.push({
          source: u,
          target: v,
          line:
            importLine(R, u, v, l => l === 'deferred') ?? evidenceLine(R, u, v),
        });
      if (load === 'unknown')
        unsettled.push({
          source: u,
          target: v,
          kinds: evidenceOf(R, u, v).kinds,
        });
    }
  }

  const violations: Violation[] = [];
  for (const arg of forbid) {
    const hits = forbidden(R, arg, reached, packages);
    for (const target of hits) {
      const chain =
        target.startsWith('pkg:') && !R.file.has(target)
          ? (packages.find(p => p.id === target)?.chain ?? [target])
          : chainOf(target);
      violations.push({
        forbidden: arg,
        target,
        load:
          target.startsWith('pkg:') && !R.file.has(target)
            ? (packages.find(p => p.id === target)?.load ?? 'eager')
            : loadOfFile(target),
        chain: chainHops(R, chain),
      });
    }
  }

  const files = sourceIds.length;
  const bytes = sum(sourceIds.map(f => bytesOf(R, f)));
  const eagerIds = sourceIds.filter(f => eager.has(f));
  const lazyIds = sourceIds.filter(f => !eager.has(f));
  const eagerBytes = sum(eagerIds.map(f => bytesOf(R, f)));
  const lazyBytes = sum(lazyIds.map(f => bytesOf(R, f)));
  const external = packages.filter(p => p.kind === 'external');
  const builtins = packages.filter(p => p.kind === 'builtin');
  const workspace = packages.filter(p => p.kind === 'workspace');
  const own = packages
    .filter(p => p.kind === 'source' && p.bytes != null)
    .slice(0, 3)
    .map(p => `${p.id} ${formatBytes(p.bytes)}`);
  // Installed packages the build measured, heaviest first: their bytes are part of what the entry loads.
  const measured = external
    .filter(p => p.bytes != null && p.bytesMethod)
    .toSorted((a, b) => (b.bytes ?? 0) - (a.bytes ?? 0));
  const packageBytesTotal = measured.reduce((a, p) => a + (p.bytes ?? 0), 0);
  const label = seeds.join(', ');
  const what = eagerOnly ? 'loads at startup' : 'loads';
  const split =
    eagerOnly || !lazyIds.length
      ? ''
      : ` (eager ${plural(eagerIds.length, 'file')} ${formatBytes(eagerBytes)}, lazy ${plural(lazyIds.length, 'file')} ${formatBytes(lazyBytes)} behind ${plural(boundaries.length, 'dynamic import')})`;
  const notes = [
    workspace.length
      ? `${plural(workspace.length, 'workspace package')} linked as one node each (bytesIfWhole is the most it adds; reach its entry file for what it loads)`
      : '',
    measured.length
      ? `imported packages: ${measured
          .slice(0, 3)
          .map(p => `${p.id} ${formatBytes(p.bytes)} (${p.bytesMethod})`)
          .join(', ')}`
      : '',
    external.length > measured.length
      ? `${plural(external.length - measured.length, 'external package')} unmeasured (rebuild to measure installed packages, or attach a bundler metafile)`
      : '',
    unsettled.length
      ? `${plural(unsettled.length, 'walked import')} with no established load counted as loading (unsettled)`
      : '',
    bytes == null ? 'some files have no recorded size' : '',
    rows.length
      ? `each row names the file that pulls it in (via); the whole chain to one: path <entry> <file> --${mode}`
      : '',
  ].filter(Boolean);
  const broken = violations.length
    ? `RULE BROKEN: ${label} reaches ${plural(violations.length, 'forbidden target')} (${[...new Set(violations.map(v => v.target))].join(', ')}); `
    : forbid.length
      ? `rule holds: ${label} reaches none of ${forbid.join(', ')}; `
      : '';
  return {
    rows,
    edges,
    nodes: [...seeds, ...rows.map(r => r.id)],
    packages,
    boundaries,
    unsettled,
    ...(forbid.length && { violations }),
    ...(violations.length && { failed: true }),
    stats: {
      load: mode,
      files,
      bytes,
      eagerFiles: eagerIds.length,
      eagerBytes,
      lazyFiles: lazyIds.length,
      lazyBytes,
      packages: packages.length,
      external: external.length,
      packageBytes: packageBytesTotal,
      packagesMeasured: measured.length,
      builtin: builtins.length,
      workspace: workspace.length,
      boundaries: boundaries.length,
      unsettled: unsettled.length,
      entryBytes: sum(seeds.map(s => bytesOf(R, s))),
    },
    summary: `${broken}${label} ${what} ${plural(files, 'file')} ${formatBytes(bytes)}${split}${own.length ? `; own code: ${own.join(', ')}` : ''}; ${plural(external.length + workspace.length, 'imported package')} (${external.length} external, ${workspace.length} workspace)${builtins.length ? ` plus ${plural(builtins.length, 'builtin module')}` : ''}${notes.length ? `; ${notes.join('; ')}` : ''}`,
  };
}

// The files an entry argument names: a file (any form a query accepts) or a directory, every file under it.
function entryFiles(R: QueryIndex, arg: string) {
  if (R.file.has(arg)) return [arg];
  const dir = `${arg.replace(/^\.\//, '').replace(/\/+$/, '')}/`;
  const under = R.ids.filter(
    id => id.startsWith(dir) && !id.startsWith('pkg:')
  );
  return under.length ? under : [resolveFile(R, arg)];
}

// The reached files and packages a forbid argument names: a pkg:<name> (node or installed package), a directory (every
// file under it) or a file. A path that names nothing in the graph is an error: a misspelled rule must not pass.
function forbidden(
  R: QueryIndex,
  arg: string,
  reached: Set<string>,
  packages: PackageRow[]
) {
  if (arg.startsWith('pkg:')) {
    if (
      reached.has(arg) ||
      packages.some(p => p.id === arg && p.kind !== 'source')
    )
      return [arg];
    // A package nothing imports would make the rule hold forever: a misspelled name must not pass.
    const known = knownPackages(R);
    if (!known.includes(arg))
      throw new QueryError(
        `--forbid ${arg} names no package this graph imports; check the name (a rule against a package nothing imports always holds)`,
        { candidates: nearest(arg, known).best.toSorted(), arg }
      );
    return [];
  }
  const dir = `${arg.replace(/^\.\//, '').replace(/\/+$/, '')}/`;
  const under = R.ids.filter(id => id.startsWith(dir));
  if (under.length) return under.filter(id => reached.has(id));
  const id = resolveFile(R, arg);
  return reached.has(id) ? [id] : [];
}

export function why(I: QueryIndex, q: QueryArgs): OpResult {
  if (q.args.length !== 2)
    throw new QueryError(
      'why needs an entry file and a target: why <entry> <file|pkg:name> [--eager]'
    );
  const mode: LoadMode = I.load === 'eager' ? 'eager' : 'runtime';
  const R = indexOf(I.D, { tests: I.tests, load: mode });
  const entry = resolveFile(R, q.args[0]);
  const arg = q.args[1];
  const external = arg.startsWith('pkg:') && !R.file.has(arg);
  // An installed package is no node of the file graph: a route ends at a file that imports it.
  const ends = (X: QueryIndex, load: LoadMode | null) =>
    external ? packageImporters(X, arg, load) : [resolveFile(X, arg)];
  const target = external ? arg : ends(R, mode)[0];
  if (target === entry)
    throw new QueryError('why needs a target other than the entry');
  const routesIn = (X: QueryIndex, load: LoadMode | null) => {
    const files = ends(X, load);
    const goals = new Set(files.map(f => X.G.index.get(f) ?? -1));
    const { routes } = firstHopRoutes(X, entry, goals);
    // The entry itself importing the package is the route with no file between them.
    const found =
      external && files.includes(entry)
        ? [{ path: [entry], cost: 0 }, ...routes]
        : routes;
    return external
      ? found.map(r => ({ ...r, path: [...r.path, arg] }))
      : found;
  };
  const found = routesIn(R, mode);
  const at = mode === 'eager' ? ' at startup' : ' at run time';
  if (!found.length) {
    // Over every edge (type-only imports too) a route may exist that never runs: say so.
    const anyEdge = routesIn(indexOf(I.D, { tests: I.tests }), null).length;
    return {
      found: false,
      rows: [],
      edges: [],
      nodes: [entry, ...(external ? [] : [target])],
      stats: { load: mode, routes: 0 },
      summary: `${entry} does not load ${target}${at}${anyEdge ? `; it reaches it only through imports that ${mode === 'eager' ? 'do not run at startup (import(), import type)' : 'never run (import type)'}: path ${entry} ${target} shows that route` : ''}`,
    };
  }
  const chains = found.map(r => chainHops(R, r.path));
  const rows: Row[] = found.map((r, i) => ({
    ...fileRow(R, r.path[1], {
      route: i + 1,
      hops: r.path.length - 1,
      path: r.path,
      chain: chains[i],
    }),
    loc: { path: entry, line: chains[i][0].line ?? 1 },
  }));
  const strongest = found.reduce((best, r) => (r.cost < best.cost ? r : best));
  const files = (path: string[]) => (external ? path.slice(0, -1) : path);
  const edges = [
    ...new Set(
      found.flatMap(r =>
        files(r.path)
          .slice(1)
          .map((id, i) => `${r.path[i]}\0${id}`)
      )
    ),
  ].map(k => k.split('\0') as Pair);
  return {
    found: true,
    rows,
    edges,
    nodes: [...new Set(found.flatMap(r => files(r.path)))],
    stats: { load: mode, routes: found.length },
    summary: `${entry} loads ${target}${at} through ${plural(found.length, 'distinct first hop')}; strongest route: ${hopsText(chainHops(R, strongest.path))}; each row is the strongest route through one first hop, each hop with its import line and kinds`,
  };
}
