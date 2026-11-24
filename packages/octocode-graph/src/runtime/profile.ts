// hot [files|functions] [n|all] [--from <file>] [--heap|--cpu]: where a profile's cost lands in the graph, from the
// SAMPLED relations `enrich` attached (profile-ingest.ts). Rows rank by self cost with their total (cost below them,
// once per stack); `--from` keeps the runtime reach of a file (`reach <file> --runtime`): the hot code of one flow.
// Cost no graph file holds is stated by reason, package and file, never dropped. Pure over data.json.
import type { Relation } from '../relations.ts';
import { runtimeReach } from './bundle.ts';
import { QueryError, type QueryIndex } from './query-index.ts';
import {
  plural,
  type OpResult,
  type QueryArgs,
  type Row,
} from './query-ops.ts';
import { formatBytes } from './query-group.ts';

type Detailed = Relation & { artifact?: string; details?: unknown };
type Level = 'files' | 'functions';
type Unit = 'us' | 'bytes';

const FORMATS = { cpu: 'cpuprofile', heap: 'heapprofile' } as const;
const byText = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);

/** A cost for people: µs, ms or s of CPU time; B, KiB or MiB of sampled memory. */
export function formatCost(n: number, unit: Unit) {
  if (unit === 'bytes') return formatBytes(Math.round(n));
  if (n < 1000) return `${Math.round(n)} µs`;
  if (n < 1e6) return `${(n / 1000).toFixed(1)} ms`;
  return `${(n / 1e6).toFixed(2)} s`;
}

interface Hot {
  id: string;
  file: string;
  line: number | null;
  self: number;
  total: number;
  name?: string;
  symbol?: string | null;
  kind?: string;
}

function parse(args: string[]) {
  let level: Level = 'files',
    n: number | null = 20,
    from: string | null = null,
    kind: 'cpu' | 'heap' | null = null;
  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    if (a === '--from') {
      from = args[++i] ?? null;
      if (!from) throw new QueryError('--from needs a file');
    } else if (a === '--heap' || a === '--cpu')
      kind = a === '--heap' ? 'heap' : 'cpu';
    else if (/^(files?)$/i.test(a)) level = 'files';
    else if (/^(functions?|fns?)$/i.test(a)) level = 'functions';
    else if (/^all$/i.test(a)) n = null;
    else if (/^\d+$/.test(a) && Number(a) > 0) n = Number(a);
    else
      throw new QueryError(
        `hot [files|functions] [n|all] [--from <file>] [--heap|--cpu]: "${a}" is none of these`
      );
  }
  return { level, n, from, kind };
}

const sumInto = (into: Record<string, number>, from: unknown) => {
  for (const [k, v] of Object.entries((from ?? {}) as Record<string, number>))
    into[k] = (into[k] ?? 0) + v;
};

/** `hot`: files or functions ranked by self cost, with total, from every profile of one kind. */
export function hot(I: QueryIndex, q: QueryArgs): OpResult {
  const { level, n, from, kind: asked } = parse(q.args);
  const providers = I.D.enrichment?.providers ?? [];
  const of = (k: 'cpu' | 'heap') =>
    providers.filter(p => p.format === FORMATS[k]);
  if (!of('cpu').length && !of('heap').length)
    throw new QueryError(
      'no profile evidence in this graph: attach a profile with `octocode-graph enrich` (formats cpuprofile, heapprofile)'
    );
  const kind = asked ?? (of('cpu').length ? 'cpu' : 'heap');
  const profiles = of(kind);
  if (!profiles.length)
    throw new QueryError(
      `no ${FORMATS[kind]} in this graph's evidence; ${kind === 'cpu' ? '--heap' : '--cpu'} answers from the other kind`
    );
  const unit: Unit = kind === 'cpu' ? 'us' : 'bytes';
  const artifacts = new Set(profiles.map(p => String(p.file)));
  const want = level === 'files' ? 'file' : 'function';
  const merged = new Map<string, Hot>();
  for (const r of (I.D.enrichment?.relations ?? []) as Detailed[]) {
    if (r.relation !== 'SAMPLED' || !artifacts.has(r.artifact ?? '')) continue;
    const d = (r.details ?? {}) as Record<string, unknown>;
    if (d.level !== want) continue;
    const h = merged.get(r.target) ?? {
      id: r.target,
      file: r.path,
      line: r.lines[0] ?? null,
      self: 0,
      total: 0,
      ...(want === 'function' && {
        name: typeof d.name === 'string' ? d.name : '',
        symbol: typeof d.symbol === 'string' ? d.symbol : null,
        ...(typeof d.kind === 'string' && { kind: d.kind }),
      }),
    };
    h.self += Number(d.self ?? 0);
    h.total += Number(d.total ?? 0);
    merged.set(r.target, h);
  }

  let all = [...merged.values()];
  let reach: ReturnType<typeof runtimeReach> | null = null;
  let outsideReach = 0;
  if (from) {
    reach = runtimeReach(I, from);
    const inside = reach.files;
    outsideReach = all
      .filter(h => !inside.has(h.file))
      .reduce((a, h) => a + h.self, 0);
    all = all.filter(h => inside.has(h.file));
  }
  all.sort(
    (a, b) => b.self - a.self || b.total - a.total || byText(a.id, b.id)
  );
  const shown = n === null ? all : all.slice(0, n);

  const stats = {
    profiles: [...artifacts],
    unit,
    sampled: 0,
    idle: 0,
    mapped: 0,
    unmapped: {} as Record<string, number>,
    unmappedPackages: {} as Record<string, number>,
    unmappedFiles: {} as Record<string, { reason: string; self: number }>,
  };
  for (const p of profiles) {
    stats.sampled += Number(p.sampled ?? 0);
    stats.idle += Number(p.idle ?? 0);
    stats.mapped += Number(p.mapped ?? 0);
    sumInto(stats.unmapped, p.unmapped);
    sumInto(stats.unmappedPackages, p.unmappedPackages);
    for (const [file, f] of Object.entries(
      (p.unmappedFiles ?? {}) as Record<
        string,
        { reason: string; self: number }
      >
    )) {
      const into = (stats.unmappedFiles[file] ??= {
        reason: f.reason,
        self: 0,
      });
      into.self += f.self;
    }
  }
  const busy = stats.sampled - stats.idle;
  const pct = (x: number) =>
    busy ? Number(((100 * x) / busy).toFixed(1)) : null;
  const fmt = (x: number) => formatCost(x, unit);
  const rows: Row[] = shown.map((h, k) => ({
    id: h.file,
    label: h.name ?? h.file.slice(h.file.lastIndexOf('/') + 1),
    chunk: I.file.get(h.file)?.chunk ?? null,
    loc: { path: h.file, line: h.line },
    role: level === 'files' ? 'file' : 'function',
    rank: k + 1,
    ...(h.symbol && {
      symbol: h.symbol,
      name: h.name,
      kind: h.kind ?? 'symbol',
    }),
    ...(level === 'functions' && !h.symbol && { frame: h.name, key: h.id }),
    self: h.self,
    total: h.total,
    unit,
    share: pct(h.self),
    totalShare: pct(h.total),
    cost: `self ${fmt(h.self)} (${pct(h.self)?.toFixed(1) ?? '-'}%) · total ${fmt(h.total)} (${pct(h.total)?.toFixed(1) ?? '-'}%)${level === 'functions' && !h.symbol ? ` · no graph symbol holds this line; frame ${h.name || '(anonymous)'}` : ''}`,
  }));
  const reasons = Object.entries(stats.unmapped)
    .sort(([a, x], [b, y]) => y - x || byText(a, b))
    .map(([reason, cost]) => `${reason} ${fmt(cost)}`);
  const packages = Object.entries(stats.unmappedPackages)
    .sort(([a, x], [b, y]) => y - x || byText(a, b))
    .map(([pkg, cost]) => `${pkg} ${fmt(cost)}`);
  const unmappedFiles = Object.keys(stats.unmappedFiles).length;
  const flow = reach
    ? ` in \`${reach.query}\` (${plural(reach.files.size, 'file')}; ${fmt(outsideReach)} of mapped self cost lies outside it)`
    : '';
  const cut =
    shown.length < all.length
      ? `; \`hot ${level} all${from ? ` --from ${from}` : ''}${asked ? ` --${kind}` : ''}\` lists every one`
      : '';
  return {
    rows,
    nodes: [...new Set(shown.map(h => h.file))].filter(id => I.file.has(id)),
    edges: [],
    stats: {
      ...stats,
      busy,
      rows: all.length,
      ...(reach && {
        reach: {
          query: reach.query,
          files: reach.files.size,
          unsettled: reach.unsettled,
        },
        outsideReach,
      }),
    },
    summary: `${kind === 'cpu' ? 'CPU' : 'Heap'} ${level} by self ${kind === 'cpu' ? 'time' : 'bytes'}${flow}: ${shown.length} of ${plural(all.length, level === 'files' ? 'file' : 'function')}${cut}. ${fmt(stats.sampled)} sampled from ${plural(artifacts.size, 'profile')}${stats.idle ? `, ${fmt(stats.idle)} idle` : ''}; ${fmt(stats.mapped)} (${pct(stats.mapped)?.toFixed(1) ?? '-'}% of ${kind === 'cpu' ? 'busy time' : 'bytes'}) maps to graph files; unmapped: ${reasons.join(', ') || 'none'}${packages.length ? `; packages: ${packages.join(', ')}` : ''}${unmappedFiles ? `; ${plural(unmappedFiles, 'file')} outside the graph in stats.unmappedFiles` : ''}`,
  };
}
