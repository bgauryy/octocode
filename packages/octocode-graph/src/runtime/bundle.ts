// bundle [<output>] [--diff]: what a bundler shipped, from the bundle relations `enrich` attached (bundle-ingest.ts).
// Without an output: every output with its bytes and entry. With one: its bytes by source file and by package,
// heaviest first. `--diff` sets the output (and the chunks it loads statically) against the source graph's runtime
// reach from its entry (`reach <entry> --runtime`): reached files it does not ship, and shipped files the reach lacks.
// Pure over data.json, like every op: the offline viewer's worker runs it too.
import type { Relation } from '../relations.ts';
import { nearest } from './query-forgiving.ts';
import {
  QueryError,
  indexOf,
  resolveFile,
  type QueryIndex,
} from './query-index.ts';
import {
  plural,
  type OpResult,
  type QueryArgs,
  type Row,
} from './query-ops.ts';
import { bytesOf, formatBytes, packageOf } from './query-group.ts';
import { reach } from './query-reach.ts';

type Detailed = Relation & { artifact?: string; details?: unknown };

interface Output {
  id: string;
  artifact: string;
  bytes: number | null;
  entry: string | null;
  kind: string;
  measure: 'output' | 'module';
}

interface Bundles {
  outputs: Map<string, Output>;
  /** Output → its inputs and their bytes in it. */
  inputs: Map<string, { id: string; bytes: number }[]>;
  loads: Map<string, { output: string; kind: string }[]>;
  /** Every input a bundler read, with its own size (the largest any artifact reports). */
  parsed: Map<string, number>;
  /** Input → the inputs that import it, in the bundler's module graph. */
  importers: Map<string, Set<string>>;
  /** Output or input → the specifiers it leaves to the runtime. */
  externals: Map<string, Set<string>>;
}

const detailsOf = (r: Detailed) =>
  (r.details && typeof r.details === 'object' ? r.details : {}) as Record<
    string,
    unknown
  >;
const after = (id: string, prefix: string) =>
  id.startsWith(prefix) ? id.slice(prefix.length) : id;
const byText = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);
const cache = new WeakMap<object, Bundles>();

function add<V>(map: Map<string, V[]>, key: string, value: V) {
  const items = map.get(key);
  if (items) items.push(value);
  else map.set(key, [value]);
}

export const BUNDLE_FORMATS = [
  'esbuild-metafile',
  'rollup-bundle',
  'rolldown-analyze',
  'webpack-stats',
];

function bundlesOf(I: QueryIndex): Bundles {
  const relations = (I.D.enrichment?.relations ?? []) as Detailed[];
  const known = cache.get(relations);
  if (known) return known;
  const B: Bundles = {
    outputs: new Map(),
    inputs: new Map(),
    loads: new Map(),
    parsed: new Map(),
    importers: new Map(),
    externals: new Map(),
  };
  for (const r of relations) {
    const d = detailsOf(r);
    if (r.relation === 'OUTPUTS') {
      const id = after(r.target, 'bundle:');
      const other = B.outputs.get(id);
      if (other)
        throw new QueryError(
          `output ${id} is in two bundle artifacts (${other.artifact}, ${r.artifact ?? r.path}): enrich one build per export`
        );
      B.outputs.set(id, {
        id,
        artifact: r.artifact ?? r.path,
        bytes: typeof d.bytes === 'number' ? d.bytes : null,
        entry: typeof d.entry === 'string' ? d.entry : null,
        kind: typeof d.kind === 'string' ? d.kind : 'chunk',
        measure: d.measure === 'module' ? 'module' : 'output',
      });
    } else if (r.relation === 'BUNDLES')
      add(B.inputs, after(r.source, 'bundle:'), {
        id: r.target,
        bytes: Number(d.bytes ?? 0),
      });
    else if (r.relation === 'LOADS')
      add(B.loads, after(r.source, 'bundle:'), {
        output: after(r.target, 'bundle:'),
        kind: d.kind === 'dynamic' ? 'dynamic' : 'static',
      });
    else if (r.relation === 'INPUTS')
      B.parsed.set(
        r.target,
        Math.max(B.parsed.get(r.target) ?? 0, Number(d.bytes ?? 0))
      );
    else if (r.relation === 'BUNDLE_IMPORTS') {
      const set = B.importers.get(r.target) ?? new Set();
      set.add(r.source);
      B.importers.set(r.target, set);
    } else if (r.relation === 'EXTERNAL') {
      const from = after(r.source, 'bundle:');
      const set = B.externals.get(from) ?? new Set();
      set.add(after(r.target, 'external:'));
      B.externals.set(from, set);
    }
  }
  cache.set(relations, B);
  return B;
}

/** The package an input belongs to: an installed package by its node_modules name, else the graph's workspace
 * package (query-group.ts packageOf), else the deepest chunk holding the path; `(virtual)` and `(outside root)`. */
export function inputPackage(I: QueryIndex, id: string) {
  const nm = id.lastIndexOf('node_modules/');
  if (nm >= 0) {
    const parts = id.slice(nm + 'node_modules/'.length).split('/');
    return `pkg:${parts[0].startsWith('@') ? parts.slice(0, 2).join('/') : parts[0]}`;
  }
  if (I.file.has(id)) return packageOf(I, id);
  if (
    id.startsWith('\\0') ||
    id.startsWith('<') ||
    id.startsWith('(disabled)') ||
    /^[a-z][\w+.-]*:/i.test(id)
  )
    return '(virtual)';
  if (id === '..' || id.startsWith('../')) return '(outside root)';
  const chunk = (I.D.chunks ?? [])
    .filter(c => c.path === '.' || id.startsWith(`${c.path}/`))
    .sort((a, b) => b.path.length - a.path.length)[0];
  if (chunk) return chunk.package ? `pkg:${chunk.package}` : chunk.id;
  return id.split('/')[0];
}

function outputOf(B: Bundles, arg: string): Output {
  const name = after(arg, 'bundle:');
  const exact = B.outputs.get(name);
  if (exact) return exact;
  const hits = [...B.outputs.keys()].filter(o => o.endsWith(`/${name}`));
  if (hits.length === 1) return B.outputs.get(hits[0])!;
  if (hits.length)
    throw new QueryError(`output ${arg} is ambiguous`, {
      candidates: hits.sort(byText),
      arg,
    });
  const base = (o: string) => o.slice(o.lastIndexOf('/') + 1);
  const near = nearest(base(name), [...B.outputs.keys()].map(base)).best;
  const candidates = [...B.outputs.keys()]
    .filter(o => near.includes(base(o)))
    .sort(byText);
  throw new QueryError(
    `no output ${arg}${candidates.length ? '' : `; outputs: ${[...B.outputs.keys()].sort(byText).join(', ')}`}`,
    { candidates, arg: candidates.length ? arg : null }
  );
}

// The outputs that load with `start`: it and every chunk reached by static imports; then those behind import().
function closure(B: Bundles, start: string) {
  const eager = new Set([start]);
  for (const o of eager)
    for (const l of B.loads.get(o) ?? [])
      if (l.kind === 'static') eager.add(l.output);
  const lazy = new Set<string>();
  const queue = [...eager];
  for (const o of queue)
    for (const l of B.loads.get(o) ?? [])
      if (!eager.has(l.output) && !lazy.has(l.output)) {
        lazy.add(l.output);
        queue.push(l.output);
      }
  return { eager: [...eager], lazy: [...lazy] };
}

const share = (part: number, whole: number | null) =>
  whole ? Number(((100 * part) / whole).toFixed(1)) : null;
const MEASURE = {
  output: 'bytes in output',
  module:
    'module size before rendering (analyze-data.json records no bytes in output)',
};

function listOutputs(I: QueryIndex, B: Bundles): OpResult {
  const outputs = [...B.outputs.values()].sort(
    (a, b) => (b.bytes ?? -1) - (a.bytes ?? -1) || byText(a.id, b.id)
  );
  const rows: Row[] = outputs.map((o, k) => ({
    id: o.id,
    label: o.id.slice(o.id.lastIndexOf('/') + 1),
    loc: { path: o.id, line: null },
    role: o.kind,
    rank: k + 1,
    bytes: o.bytes,
    entry: o.entry,
    ...(o.entry && { nextFile: o.entry }),
    inputs: B.inputs.get(o.id)?.length ?? 0,
    artifact: o.artifact,
  }));
  const total = outputs.reduce((a, o) => a + (o.bytes ?? 0), 0);
  const unknown = outputs.filter(o => o.bytes === null).length;
  return {
    rows,
    nodes: [
      ...new Set(
        outputs.flatMap(o => (o.entry && I.file.has(o.entry) ? [o.entry] : []))
      ),
    ],
    edges: [],
    stats: { outputs: outputs.length, bytes: total, sizeUnknown: unknown },
    summary: `${plural(outputs.length, 'output')}, ${formatBytes(total)}${unknown ? ` (+${unknown} of unknown size)` : ''}, heaviest first; \`bundle <output>\` breaks one down`,
  };
}

function breakdown(I: QueryIndex, B: Bundles, o: Output): OpResult {
  const inputs = (B.inputs.get(o.id) ?? []).toSorted(
    (a, b) => b.bytes - a.bytes || byText(a.id, b.id)
  );
  const counted = inputs.reduce((a, x) => a + x.bytes, 0);
  const packages = new Map<string, { bytes: number; files: number }>();
  const files: Row[] = inputs.map((x, k) => {
    const pkg = inputPackage(I, x.id);
    const p = packages.get(pkg) ?? { bytes: 0, files: 0 };
    p.bytes += x.bytes;
    p.files++;
    packages.set(pkg, p);
    return {
      id: x.id,
      label: x.id.slice(x.id.lastIndexOf('/') + 1),
      chunk: I.file.get(x.id)?.chunk ?? null,
      loc: { path: x.id, line: null },
      role: 'file',
      rank: k + 1,
      bytes: x.bytes,
      share: share(x.bytes, counted),
      package: pkg,
      inGraph: I.file.has(x.id),
    };
  });
  const groups: Row[] = [...packages]
    .sort(([a, x], [b, y]) => y.bytes - x.bytes || byText(a, b))
    .map(([id, p]) => ({
      id,
      label: id,
      loc: null,
      group: 'package',
      count: p.files,
      bytes: p.bytes,
      share: share(p.bytes, counted),
    }));
  const overhead =
    o.measure === 'output' && o.bytes !== null ? o.bytes - counted : null;
  const externals = new Set(B.externals.get(o.id) ?? []);
  for (const x of inputs)
    for (const e of B.externals.get(x.id) ?? []) externals.add(e);
  const loads = B.loads.get(o.id) ?? [];
  return {
    rows: [...files, ...groups],
    nodes: inputs.map(x => x.id).filter(id => I.file.has(id)),
    edges: [],
    stats: {
      output: o.id,
      artifact: o.artifact,
      entry: o.entry,
      bytes: o.bytes,
      measure: o.measure,
      inputs: inputs.length,
      inputBytes: counted,
      overhead,
      packages: packages.size,
      loads,
      externals: [...externals].sort(byText),
    },
    summary: `${o.id}${o.entry ? ` (entry ${o.entry})` : ''}: ${formatBytes(o.bytes)}; ${plural(inputs.length, 'source file')} in ${plural(packages.size, 'package')}, ${formatBytes(counted)} of ${MEASURE[o.measure]}${overhead !== null ? `, ${formatBytes(overhead)} bundler code and padding` : ''}${loads.length ? `; loads ${loads.map(l => `${l.output} (${l.kind})`).join(', ')}` : ''}; heaviest first`,
  };
}

/**
 * The runtime reach of a file: the answer of `reach <file> --runtime` as sets (files, packages) and its rows by file.
 * The reach op runs here as runQuery runs it for that text (query-reach.ts), without the query reader in between.
 */
export function runtimeReach(I: QueryIndex, file: string) {
  const entry = resolveFile(I, file);
  const quoted = /[\s"']/.test(entry) ? JSON.stringify(entry) : entry;
  const query = `reach ${quoted} --runtime${I.tests ? ' --tests' : ''}`;
  const R = indexOf(I.D, { tests: I.tests, load: 'runtime' });
  const r = reach(R, {
    args: [entry],
    tests: I.tests,
    imports: false,
    weak: false,
    load: 'runtime',
    depth: null,
  });
  const rows = new Map(r.rows.map(row => [row.id, row]));
  const files = new Set(
    [entry, ...rows.keys()].filter(id => !id.startsWith('pkg:'))
  );
  const packages = new Set(
    [...rows.keys()].filter(id => id.startsWith('pkg:'))
  );
  for (const p of (r.packages ?? []) as { id: string; kind: string }[])
    if (p.kind !== 'source') packages.add(p.id);
  // Imports whose load the graph could not settle count as loading: the reach may hold more than runs.
  const unsettled = Array.isArray(r.unsettled) ? r.unsettled.length : 0;
  return { entry, query, files, rows, packages, unsettled };
}

function diff(I: QueryIndex, B: Bundles, o: Output): OpResult {
  if (!o.entry)
    throw new QueryError(
      `output ${o.id} has no entry point: --diff compares an entry's output with the entry's reach; \`bundle\` lists the entries`
    );
  if (!I.file.has(o.entry))
    throw new QueryError(
      `the entry ${o.entry} of ${o.id} is not a file of this graph`
    );
  const flow = runtimeReach(I, o.entry);
  const { eager, lazy } = closure(B, o.id);
  // Where each shipped input lives: the outputs that load with this one, then those behind import().
  const shipped = new Map<string, { bytes: number; output: string }>();
  const later = new Map<string, { bytes: number; output: string }>();
  for (const [into, outputs] of [
    [shipped, eager],
    [later, lazy],
  ] as const)
    for (const out of outputs)
      for (const x of B.inputs.get(out) ?? [])
        if (x.bytes > 0 && !shipped.has(x.id) && !into.has(x.id))
          into.set(x.id, { bytes: x.bytes, output: out });
  const rows: Row[] = [];
  const row = (id: string, extra: Record<string, unknown>): Row => ({
    id,
    label: id.slice(id.lastIndexOf('/') + 1),
    chunk: I.file.get(id)?.chunk ?? null,
    loc: { path: id, line: null },
    ...extra,
  });
  for (const id of [...flow.files].sort(byText)) {
    if (shipped.has(id) || later.has(id)) continue;
    const r = flow.rows.get(id);
    const read = B.parsed.has(id);
    rows.push(
      row(id, {
        role: read ? 'tree-shaken' : 'not shipped',
        diff: 'dropped',
        ...(r?.load !== undefined && { load: r.load }),
        bytes: bytesOf(I, id) ?? B.parsed.get(id) ?? null,
        ...(r?.through !== undefined && { through: r.through }),
        ...(r?.importedAt !== undefined && { importedAt: r.importedAt }),
      })
    );
  }
  const holder = (id: string) => {
    const pkg = inputPackage(I, id);
    return flow.packages.has(pkg) ? pkg : null;
  };
  for (const [where, map] of [
    ['static', shipped],
    ['dynamic', later],
  ] as const)
    for (const [id, x] of map) {
      if (flow.files.has(id)) continue;
      const pkg = holder(id);
      const importedBy = [...(B.importers.get(id) ?? [])]
        .filter(f => shipped.has(f) || later.has(f))
        .sort(byText);
      rows.push(
        row(id, {
          role: pkg ? 'package' : 'unexpected',
          diff: pkg ? 'package' : 'unexpected',
          bytes: x.bytes,
          ...(importedBy.length && { through: importedBy[0] }),
          importedBy,
          output: x.output,
          loads: where,
          package: inputPackage(I, id),
        })
      );
    }
  const order = { dropped: 0, unexpected: 1, package: 2 } as const;
  rows.sort(
    (a, b) =>
      order[a.diff as keyof typeof order] -
        order[b.diff as keyof typeof order] ||
      Number(b.bytes ?? 0) - Number(a.bytes ?? 0) ||
      byText(a.id, b.id)
  );
  const of = (kind: string) => rows.filter(r => r.diff === kind);
  const bytes = (rs: Row[]) => rs.reduce((a, r) => a + Number(r.bytes ?? 0), 0);
  const [dropped, unexpected, packaged] = [
    of('dropped'),
    of('unexpected'),
    of('package'),
  ];
  const shaken = dropped.filter(r => r.role === 'tree-shaken').length;
  const dynamicFiles = [...later.keys()].filter(id =>
    flow.files.has(id)
  ).length;
  return {
    rows,
    nodes: [...new Set([o.entry, ...rows.map(r => r.id)])].filter(id =>
      I.file.has(id)
    ),
    edges: [],
    stats: {
      output: o.id,
      entry: o.entry,
      reach: {
        query: flow.query,
        files: flow.files.size,
        unsettled: flow.unsettled,
      },
      outputs: eager,
      dynamicOutputs: lazy,
      dropped: dropped.length,
      droppedSourceBytes: bytes(dropped),
      unexpected: unexpected.length,
      unexpectedBytes: bytes(unexpected),
      packageFiles: packaged.length,
      packageBytes: bytes(packaged),
      dynamicFiles,
    },
    summary: `${o.id} against \`${flow.query}\` (${plural(flow.files.size, 'file')}): ${dropped.length} reached but not shipped (${shaken} tree-shaken, ${dropped.length - shaken} named by no input of the build; ${formatBytes(bytes(dropped))} of source), ${unexpected.length} shipped that the reach lacks (${formatBytes(bytes(unexpected))}), ${plural(packaged.length, 'file')} of packages the reached files import (${formatBytes(bytes(packaged))})${dynamicFiles ? `; ${plural(dynamicFiles, 'reached file')} ship${dynamicFiles === 1 ? 's' : ''} in chunks loaded by import()` : ''}`,
  };
}

/** `bundle [<output>] [--diff]`. */
export function bundle(I: QueryIndex, q: QueryArgs): OpResult {
  const wantDiff = q.args.includes('--diff');
  const args = q.args.filter(a => a !== '--diff');
  const B = bundlesOf(I);
  if (!B.outputs.size)
    throw new QueryError(
      `no bundle evidence in this graph: attach a build with \`octocode-graph enrich\` (formats ${BUNDLE_FORMATS.join(', ')})`
    );
  if (args.length > 1)
    throw new QueryError('bundle takes one output: bundle [<output>] [--diff]');
  if (!args.length) {
    if (wantDiff)
      throw new QueryError('--diff needs an output: bundle <output> --diff');
    return listOutputs(I, B);
  }
  const o = outputOf(B, args[0]);
  return wantDiff ? diff(I, B, o) : breakdown(I, B, o);
}
