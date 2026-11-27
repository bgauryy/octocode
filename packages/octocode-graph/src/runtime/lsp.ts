// Compare the stitched graph with call hierarchy from the selected declarations: each one's callees. The selection is
// the functions in hub files, the sources of calls bound between chunks, and every function holding a receiver call
// no syntax rule bound that names a method the repository declares (`client.open(..)` whose receiver type the graph
// does not know): the language server resolves that receiver inside the function. Asking a function for its callees
// resolves one body; asking a method for its callers searches the workspace for its name (measured on the native
// workspace: 4,961 callers probes took 8,076 s of call time, 6,331 callees probes 1,067 s).
// Each batch streams confirmed, new and unanswered relationships.
import { existsSync } from 'node:fs';
import { relative, resolve, join } from 'node:path';
import { errorMessage, isInteger, pool, nativeWorkspaceRoot } from './lib.ts';
import { continuations, pagedTool, readResponse } from './tools.ts';
import type { ToolData, ToolResult } from './tools.ts';
import type { ProbeCache, RawCallee } from './graph-cache.ts';
import type {
  CallEdge,
  CallStats,
  Declaration,
  Emit,
  FileNode,
  GraphData,
  LspMode,
  RunEvent,
  SymbolNode,
  UnansweredProbe,
} from './types.ts';

/** One definition an lspSearch row names, with the call sites it groups. */
interface LspMatch {
  path?: string;
  symbolName?: string;
  kind?: string;
  line?: number;
  endLine?: number;
  sites?: { line?: number }[];
}

/** A definition with a name and a line: the only ones a callee answer can place. */
type NamedMatch = LspMatch & { symbolName: string; line: number };
const isNamedAt = (m: LspMatch): m is NamedMatch =>
  Boolean(m.symbolName) && isInteger(m.line);

/** The fields read of an lspSearch row's data: its partial state and the definitions it groups by file. */
export type LspData = ToolData & {
  category?: string;
  reason?: string;
  resolvedSymbol?: { lineDeviation?: unknown };
  files?: { path: string; matches?: LspMatch[] }[];
};

/** A declaration probed for its callees: one with a line. */
type Probe = Declaration & { line: number };

/** A callee an answer names: its file, name and definition line, and the call sites that reach it. */
interface Callee {
  file: string;
  name: string;
  line: number;
  sites: number[];
}

type AmbiguousDefinition = NonNullable<
  UnansweredProbe['ambiguousDefinitions']
>[number];

/** One probe's answer: the callees it confirmed and, when the server left it incomplete, why. */
interface CalleeAnswer {
  callees?: Map<string, Callee>;
  unanswered?: string[];
  ambiguous?: AmbiguousDefinition[];
  /** The read policy hid some locations (outside the allowed roots, or a credential file by name). */
  withheld?: boolean;
  /** The server's definitions before the graph placed them: kept by the graph cache when the answer is complete. */
  raw?: RawCallee[];
}

/** The server's answer before the graph's declarations place it: incomplete (why), or every definition it named. */
type RawAnswer =
  { unanswered: string[] } | { callees: RawCallee[]; withheld: boolean };

/** Definition lines by `file#name`: the declaration id at each line, null where two declarations share it. */
type Definitions = Map<string, Map<number, string | null>>;

const lastName = (name: string | undefined) =>
  (name ?? '').split('.').pop()!.replace(/@\d+$/, '');
const keyOf = (file: string, name: string | undefined) =>
  `${file}#${lastName(name)}`;
// The tool distinguishes a complete empty answer (noLocations) from an unavailable one.
export function lspReasons(data: LspData) {
  return [
    ...new Set([
      ...(data.partialReasons ?? []),
      ...(data.errorCode ? [data.errorCode] : []),
      ...(data.category && data.category !== 'noLocations'
        ? [data.category]
        : []),
      ...(data.isPartial && !data.partialReasons?.length && !data.category
        ? ['partial']
        : []),
    ]),
  ];
}

// Each returned declaration is identified by file, name and line, and may introduce an unindexed declaration.
// Locations the read policy withheld (outside every allowed root, or a credential file by name) are never graph
// files: the graph is listed through the same policy. An answer whose only gap is withheld locations is therefore
// complete for the graph; `withheld` keeps the omission counted (callStats.withheld).
function rawCallees(rowPages: ToolResult[], base: string): RawAnswer {
  if (!rowPages.length || rowPages.some(p => !p?.data))
    return { unanswered: ['missingResult'] };
  // Every row has data (checked above): lspSearch row data.
  const rows = rowPages.map(p => p.data as LspData);
  const reasons = [...new Set(rows.flatMap(lspReasons))];
  if (rows.some(d => d.resolvedSymbol?.lineDeviation))
    return { unanswered: [...reasons, 'movedSource'] };
  const partial = rows.some(d => d.isPartial) || reasons.length > 0;
  if (
    partial &&
    (rows.some(
      d => d.errorCode || (d.isPartial && !d.partialReasons?.length)
    ) ||
      reasons.length !== 1 ||
      reasons[0] !== 'locationsWithheld')
  ) {
    return { unanswered: reasons.length ? reasons : ['partial'] };
  }
  // Past the check above, a partial answer's one reason is locationsWithheld.
  const callees: RawCallee[] = [];
  for (const d of rows)
    for (const group of d.files ?? [])
      for (const match of group.matches ?? [])
        if (isNamedAt(match))
          callees.push({
            path: resolve(base, match.path ?? group.path),
            symbolName: match.symbolName,
            line: match.line,
            sites: match.sites ?? [],
          });
  return { callees, withheld: partial };
}

// The definitions an answer named, placed on the graph's declarations by file, name and line.
function placeCallees(
  { callees: named, withheld }: { callees: RawCallee[]; withheld: boolean },
  root: string,
  definitions: Definitions
): CalleeAnswer {
  const callees = new Map<string, Callee>(),
    ambiguous: AmbiguousDefinition[] = [];
  for (const match of named) {
    const file = relative(root, match.path);
    const key = keyOf(file, match.symbolName);
    const exact = definitions.get(key)?.get(match.line);
    if (exact === null) {
      ambiguous.push({
        file,
        name: match.symbolName,
        line: match.line,
        sites: match.sites,
      });
      continue;
    }
    addCallee(callees, exact ?? `${key}@${match.line}`, file, match);
  }
  const flag = withheld ? { withheld } : {};
  if (ambiguous.length)
    return {
      callees,
      unanswered: ['ambiguousDefinition'],
      ambiguous,
      ...flag,
    };
  return { callees, ...flag };
}

function calleesOf(
  rowPages: ToolResult[],
  root: string,
  definitions: Definitions,
  base: string
): CalleeAnswer {
  const raw = rawCallees(rowPages, base);
  if ('unanswered' in raw) return raw;
  return {
    ...placeCallees(raw, root, definitions),
    // Only a complete answer is kept: withheld locations depend on the read policy of the run.
    ...(!raw.withheld && { raw: raw.callees }),
  };
}

// The language a package manifest's own server reads (the languages lspSearch serves).
const MANIFEST_SOURCES: [string, RegExp][] = [
  ['Cargo.toml', /\.rs$/],
  ['package.json', /\.[cm]?[jt]sx?$/],
  ['pyproject.toml', /\.py$/],
  ['go.mod', /\.go$/],
];

/**
 * Starts the language server of each manifest kind the chunks have (one `documentSymbols` request on one of its
 * source files), so its start and project load overlap the graph phase instead of delaying the first LSP probe.
 * Failure only means the probes start the server themselves.
 */
export async function warmLanguageServers(
  root: string,
  chunkPaths: readonly string[],
  files: readonly string[]
) {
  const queries = MANIFEST_SOURCES.flatMap(([manifest, source]) => {
    const chunk = chunkPaths.find(p => existsSync(join(root, p, manifest)));
    if (chunk === undefined) return [];
    const prefix = chunk === '.' ? '' : `${chunk}/`;
    const file = files.find(f => f.startsWith(prefix) && source.test(f));
    return file
      ? [{ operation: 'documentSymbols', path: join(root, file) }]
      : [];
  });
  if (queries.length)
    await readResponse('lspSearch', { queries }).catch(() => undefined);
}

// Merge call sites for one definition across pages.
function addCallee(
  out: Map<string, Callee>,
  key: string,
  file: string,
  m: Pick<RawCallee, 'symbolName' | 'line' | 'sites'>
) {
  const cur = out.get(key) ?? {
    file,
    name: lastName(m.symbolName),
    line: m.line,
    sites: [],
  };
  for (const line of (m.sites ?? []).map(s => s.line))
    if (isInteger(line) && !cur.sites.includes(line)) cur.sites.push(line);
  out.set(key, cur);
}

// One batch: the first response, then each row's own continuation chain, kept with its query.
async function probeBatch(
  batch: Probe[],
  root: string,
  definitions: Definitions
) {
  const base = await nativeWorkspaceRoot();
  const first = await readResponse('lspSearch', {
    queries: batch.map(x => ({
      operation: 'callees',
      path: join(root, x.file),
      symbolName: x.name,
      line: x.line,
    })),
  });
  return Promise.all(
    batch.map(async (x, i): Promise<[Probe, CalleeAnswer]> => {
      const rows = (first.results ?? []).filter(r => r.index === i);
      const tail: ToolResult[] = [];
      for (const c of continuations({ results: rows }))
        tail.push(
          ...(await pagedTool(c.tool, c.input)).flatMap(p => p.results ?? [])
        );
      return [x, calleesOf([...rows, ...tail], root, definitions, base)];
    })
  );
}

/** The graph's own rows by key, which the LSP pass reads and extends. */
interface ProbeIndex {
  files: Map<string, FileNode>;
  symbolIds: Set<string>;
  /** The graph's own symbol rows by id, as the pass started. */
  nodes: Map<string, SymbolNode>;
  calls: Map<string, CallEdge[]>;
  definitions: Definitions;
}

/** The running counts of an LSP pass; every unanswered probe is listed. */
interface LspStats {
  lsp: string;
  probed: number;
  resolved: number;
  partialWithMatches: number;
  /** Answered probes whose answer omitted locations the read policy withheld. */
  withheld: number;
  both: number;
  graphOnly: number;
  lspOnly: number;
  /** Sources selected only for a receiver call no rule bound (receiverCallSources), counted in `total` too. */
  receiverSources: number;
  /** Sources a refresh carried over instead of probing again (carriedProbes). */
  carried: number;
  /** Declarations answered from the shared graph cache (graph-cache.ts) instead of a probe; counted in `resolved`. */
  cached: number;
  unanswered: UnansweredProbe[];
  scope: string;
  total?: number;
}

/** A call edge the LSP pass adds: `test` is set only when its caller is test code. */

/** The `lsp` event: one batch's confirmed and added calls, the running counts and the probes newly unanswered. */
export interface LspEvent extends RunEvent {
  type: 'lsp';
  update: { source: string; target: string; via: CallEdge['via'] }[];
  added: CallEdge[];
  probed: number;
  resolved: number;
  total: number;
  stats: Omit<LspStats, 'unanswered'>;
  unansweredAdded: UnansweredProbe[];
}

// `answer.callees` is set: applyCallees runs only for answers with callees.
function applyCallees(
  D: GraphData,
  x: Probe,
  answer: CalleeAnswer,
  index: ProbeIndex,
  stats: LspStats
) {
  const update: LspEvent['update'] = [],
    added: CallEdge[] = [];
  const L = answer.callees!;
  const mine: CallEdge[] = index.calls.get(x.id) ?? [];
  const seen = new Set<string>();
  for (const c of mine) {
    seen.add(c.target);
    const hit = L.has(c.target);
    if (!hit && answer.unanswered) continue;
    c.via = hit ? 'both' : c.via === 'lsp' ? 'lsp' : 'graph';
    stats[hit ? 'both' : 'graphOnly']++;
    update.push({ source: c.source, target: c.target, via: c.via });
  }
  for (const [id, def] of L) {
    if (seen.has(id) || !index.files.has(def.file)) continue;
    if (!index.symbolIds.has(x.id)) {
      index.symbolIds.add(x.id);
      D.symbols.push({ ...x, calls: 0 });
    }
    stats.lspOnly++;
    const lines = def.sites.toSorted((a, b) => a - b);
    const e: CallEdge = {
      source: x.id,
      target: id,
      line: lines[0] ?? null,
      lines,
      count: Math.max(1, lines.length),
      confidence: 'high',
      resolution: ['lsp-callee'],
      via: 'lsp',
    };
    if (index.files.get(x.file)?.roles?.includes('test')) e.test = true;
    D.calls.push(e);
    mine.push(e);
    index.calls.set(x.id, mine);
    if (!index.symbolIds.has(id)) {
      index.symbolIds.add(id);
      D.symbols.push({
        id,
        file: def.file,
        name: def.name,
        kind: 'symbol',
        line: def.line,
        calls: 0,
        exported: false,
      });
    }
    added.push(e);
  }
  return { update, added };
}

// The functions holding a receiver call no rule bound (`client.open_document_and_wait(..)`, also the head of a chain row
// `client.open().await.expect`) whose method name the repository declares: the language server types that receiver
// when it resolves the function's own calls. `rows` carry root-relative caller ids; a name nothing here declares (a
// std method such as `len`) selects nothing, and neither does a receiver whose spelled type (`receiverType`) no file
// here declares: a method of a std or external type (`seen: HashSet<_>`, then `seen.insert(x)`).
export function receiverCallSources(
  rows: Iterable<{ from: string; targetName: string; receiverType?: string }>,
  declared: ReadonlySet<string>,
  types: ReadonlySet<string>
) {
  const sources = new Set<string>();
  for (const { from, targetName, receiverType } of rows) {
    const head = targetName.split('(')[0];
    const member =
      /^[\p{L}_$][\p{L}\p{N}_$]*\.([\p{L}_$][\p{L}\p{N}_$]*)$/u.exec(head);
    if (!member || !declared.has(member[1])) continue;
    const type =
      receiverType && !targetName.includes('(')
        ? receiverType.split('<')[0].split('::').at(-1)!.trim()
        : null;
    if (type && !types.has(type)) continue;
    sources.add(from);
  }
  return sources;
}

// A probe answered in full: the next refresh may carry the answer over (carriedProbes).
function markProbed(D: GraphData, index: ProbeIndex, x: Declaration) {
  // A symbol this pass added (a callee it reported) is not in `nodes`: look it up once.
  let node = index.nodes.get(x.id) ?? D.symbols.find(s => s.id === x.id);
  if (!node) {
    node = { ...x, calls: 0 };
    D.symbols.push(node);
    index.symbolIds.add(x.id);
  }
  index.nodes.set(x.id, node);
  node.probed = true;
}

/** A previous run's answers a refresh keeps: the probed functions and the calls the server confirmed or added. */
export interface CarriedProbes {
  sources: ReadonlySet<string>;
  calls: readonly CallEdge[];
}

/**
 * The answers of `previous` that still hold: a function the server answered in full, whose file is not `touched` and
 * none of whose answered calls lands in a touched file (an edit there can move or remove the declaration it resolved
 * to). Everything else is probed again.
 */
export function carriedProbes(
  previous: Pick<GraphData, 'symbols' | 'calls'>,
  touched: (file: string) => boolean
): CarriedProbes {
  const answered = Map.groupBy(
    previous.calls.filter(c => c.via === 'lsp' || c.via === 'both'),
    c => c.source
  );
  const sources = new Set<string>();
  const calls: CallEdge[] = [];
  for (const s of previous.symbols) {
    if (!s.probed || touched(s.file)) continue;
    const mine = answered.get(s.id) ?? [];
    if (mine.some(c => touched(c.target.split('#')[0]))) continue;
    sources.add(s.id);
    calls.push(...mine);
  }
  return { sources, calls };
}

// Apply carried answers: a call the graph has becomes `both`, a call only the server reported is added back.
function applyCarried(
  D: GraphData,
  carried: CarriedProbes,
  index: ProbeIndex,
  stats: LspStats
) {
  const update: LspEvent['update'] = [],
    added: CallEdge[] = [];
  for (const e of carried.calls) {
    const mine = index.calls.get(e.source) ?? [];
    const known = mine.find(c => c.target === e.target);
    if (known) {
      if (known.via === 'graph') {
        known.via = 'both';
        stats.both++;
        update.push({
          source: known.source,
          target: known.target,
          via: known.via,
        });
      }
      continue;
    }
    if (e.via !== 'lsp' || !index.files.has(e.target.split('#')[0])) continue;
    const copy: CallEdge = { ...e };
    D.calls.push(copy);
    mine.push(copy);
    index.calls.set(e.source, mine);
    stats.lspOnly++;
    added.push(copy);
  }
  for (const id of carried.sources) {
    const node = index.nodes.get(id);
    if (node) node.probed = true;
  }
  stats.carried = carried.sources.size;
  return { update, added };
}

export interface LspOptions {
  root: string;
  emit: Emit;
  signal?: AbortSignal;
  declarations?: Iterable<Declaration>;
  mode?: LspMode;
  jobs?: number;
  batchSize?: number;
  /** Functions holding a receiver call no rule bound that names a method declared here (receiverCallSources). */
  receiverSources?: Iterable<string>;
  /** A refresh's answers from the previous run that still hold (carriedProbes): applied, never probed again. */
  carried?: CarriedProbes;
  /** The shared cache of answers (graph-cache.ts): answered declarations are applied, never probed again. */
  cache?: ProbeCache | null;
}

export async function lspPhase(
  D: GraphData,
  {
    root,
    emit,
    signal,
    declarations = [],
    mode = 'smart',
    jobs = 8,
    batchSize = 1,
    receiverSources = [],
    carried,
    cache,
  }: LspOptions
): Promise<CallStats> {
  // Every probe the server left unanswered is listed (never only counted), so it can be retried or reported.
  const stats: LspStats = {
    lsp: 'pending',
    probed: 0,
    resolved: 0,
    partialWithMatches: 0,
    withheld: 0,
    both: 0,
    graphOnly: 0,
    lspOnly: 0,
    receiverSources: 0,
    carried: 0,
    cached: 0,
    unanswered: [],
    scope:
      mode === 'all'
        ? 'all callable declarations'
        : 'functions in hub files, sources of calls bound between chunks, and functions holding a receiver call no rule bound to a method declared here',
  };
  const hubs = new Set(D.hubFiles);
  // Every source of a call bound between chunks, whatever its confidence: on the 3,516-file native workspace,
  // skipping the 1,221 sources bound only by exact crate paths lost 1,121 calls only the language server sees
  // (inside macros, on receivers).
  const cross = new Set(D.calls.filter(c => c.crossChunk).map(c => c.source));
  const symbols = new Map<string, Declaration>(D.symbols.map(s => [s.id, s]));
  for (const d of declarations) symbols.set(d.id, d);
  const receiver = new Set(receiverSources);
  const probe = [...symbols.values()].filter(
    (s): s is Probe =>
      isInteger(s.line) &&
      ['function', 'method', 'constructor'].includes(s.kind) &&
      (mode === 'all' ||
        hubs.has(s.file) ||
        cross.has(s.id) ||
        receiver.has(s.id)) &&
      !carried?.sources.has(s.id)
  );
  if (mode !== 'all')
    stats.receiverSources = probe.filter(
      s => !hubs.has(s.file) && !cross.has(s.id)
    ).length;
  emit({ type: 'progress', phase: 'lsp', done: 0, total: probe.length });
  if (!probe.length && !carried?.sources.size)
    return {
      ...stats,
      lsp: `skipped: no callable declarations in scope (${stats.scope})`,
    };
  stats.total = probe.length;
  const index: ProbeIndex = {
    files: new Map(D.files.map(f => [f.id, f])),
    symbolIds: new Set(D.symbols.map(s => s.id)),
    nodes: new Map(D.symbols.map(s => [s.id, s])),
    calls: Map.groupBy(D.calls, call => call.source),
    definitions: new Map(),
  };
  for (const symbol of symbols.values()) {
    if (!isInteger(symbol.line)) continue;
    const key = keyOf(symbol.file, symbol.name);
    const lines =
      index.definitions.get(key) ?? new Map<number, string | null>();
    lines.set(
      symbol.line,
      lines.has(symbol.line) && lines.get(symbol.line) !== symbol.id
        ? null
        : symbol.id
    );
    index.definitions.set(key, lines);
  }
  const size = batchSize;
  const batches: Probe[][] = [];
  // Every probe's definition lines are indexed above.
  const unique = probe.filter(x => {
    if (index.definitions.get(keyOf(x.file, x.name))!.get(x.line) !== null)
      return true;
    stats.unanswered.push({
      id: x.id,
      file: x.file,
      symbol: x.name,
      line: x.line,
      reasons: ['ambiguousSource'],
    });
    return false;
  });
  const ambiguous = probe.length - unique.length;
  const known = cache
    ? await cache.lookup(unique)
    : new Map<string, RawCallee[]>();
  const asked = unique.filter(x => !known.has(x.id));
  for (let i = 0; i < asked.length; i += size)
    batches.push(asked.slice(i, i + size));
  let sentUnanswered = 0;
  function publish(update: LspEvent['update'], added: CallEdge[]) {
    const { unanswered, ...counts } = stats;
    const unansweredAdded = unanswered.slice(sentUnanswered);
    sentUnanswered = unanswered.length;
    const event: LspEvent = {
      type: 'lsp',
      update,
      added,
      probed: stats.probed,
      resolved: stats.resolved,
      total: probe.length,
      stats: counts,
      unansweredAdded,
    };
    emit(event);
    emit({
      type: 'progress',
      phase: 'lsp',
      done: stats.probed + stats.cached + ambiguous,
      total: probe.length,
    });
  }
  if (carried?.sources.size) {
    const r = applyCarried(D, carried, index, stats);
    publish(r.update, r.added);
  } else if (ambiguous && !known.size) publish([], []);
  if (known.size) {
    const update: LspEvent['update'] = [],
      added: CallEdge[] = [];
    for (const x of unique) {
      const callees = known.get(x.id);
      if (!callees) continue;
      stats.cached++;
      const answer = placeCallees(
        { callees, withheld: false },
        root,
        index.definitions
      );
      if (answer.unanswered) {
        stats.unanswered.push({
          id: x.id,
          file: x.file,
          symbol: x.name,
          line: x.line,
          reasons: answer.unanswered,
          ...(answer.ambiguous && { ambiguousDefinitions: answer.ambiguous }),
        });
        if (!answer.callees?.size) continue;
        stats.partialWithMatches++;
      } else {
        stats.resolved++;
        markProbed(D, index, x);
      }
      const r = applyCallees(D, x, answer, index, stats);
      update.push(...r.update);
      added.push(...r.added);
    }
    publish(update, added);
  }
  // Half the cores: the language server bounds throughput (measured on 3,516 Rust files: 154 s with 6 workers,
  // 164 s with 12), and the rest stay for the server itself.
  await pool(batches, Math.max(1, Math.floor(jobs / 2)), async batch => {
    if (signal?.aborted) {
      for (const x of batch)
        stats.unanswered.push({
          id: x.id,
          file: x.file,
          symbol: x.name,
          line: x.line,
          reasons: ['cancelled'],
        });
      publish([], []);
      return;
    }
    const answers = await probeBatch(batch, root, index.definitions).catch(
      (error: unknown) =>
        batch.map((x): [Probe, CalleeAnswer] => [
          x,
          { unanswered: [`callFailed: ${errorMessage(error).split('\n')[0]}`] },
        ])
    );
    const update: LspEvent['update'] = [],
      added: CallEdge[] = [];
    for (const [x, answer] of answers) {
      stats.probed++;
      if (answer.withheld) stats.withheld++;
      if (answer.unanswered) {
        stats.unanswered.push({
          id: x.id,
          file: x.file,
          symbol: x.name,
          line: x.line,
          reasons: answer.unanswered,
          ...(answer.ambiguous && { ambiguousDefinitions: answer.ambiguous }),
        });
        if (!answer.callees?.size) continue;
        stats.partialWithMatches++;
      } else {
        stats.resolved++;
        markProbed(D, index, x);
      }
      if (answer.raw) cache?.store(x, answer.raw);
      const r = applyCallees(D, x, answer, index, stats);
      update.push(...r.update);
      added.push(...r.added);
    }
    publish(update, added);
  });
  await cache?.flush();
  D.coverage.calls = D.calls.length;
  if (stats.probed + stats.cached < unique.length) stats.lsp = 'cancelled';
  else if (!stats.unanswered.length) stats.lsp = 'ok';
  else
    stats.lsp =
      stats.resolved || stats.partialWithMatches ? 'partial' : 'unanswered';
  return { mode: 'both', ...stats };
}
