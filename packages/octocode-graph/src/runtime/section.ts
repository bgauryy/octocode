// One section (a chunk or the whole root) -> data.json: every file, import, call and declaration of its
// graph snapshot. Each graph page is handed to `emit` as it arrives, so a viewer grows while paging runs.
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, statSync } from 'node:fs';
import { appendFile, rename, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { promisify } from 'node:util';
import { gzip } from 'node:zlib';
import { basename, dirname, join, resolve } from 'node:path';
import { graphBuild, graphCall, readGraphs, totalOf } from './graph-pages.ts';
import type { BuiltGraph, GraphAnswer } from './graph-pages.ts';
import {
  SKIPPED_DIRS,
  degrees,
  errorMessage,
  isInteger,
  pool,
  prepareGraphRun,
  nativeWorkspaceRoot,
  topGroup,
} from './lib.ts';
import type {
  GraphQueryData,
  GraphQueryQuery,
  OutputGraphNodeEvidence,
} from '../contract.generated.js';
import type {
  CallEdge,
  CycleRow,
  Declaration,
  ExternalPackage,
  FileNode,
  GraphEdge,
  ImportEdge,
  Insights,
  SectionData,
  Snapshot,
  SymbolNode,
  UnresolvedCall,
} from './types.ts';
import { fileOf } from '../ids.ts';

/** A `find` (kind file) row: a file node, as the contract states node evidence. */
export type FileRow = OutputGraphNodeEvidence &
  Required<Pick<OutputGraphNodeEvidence, 'id'>>;
/** A `symbols` row: one declaration with its file, name and kind. */
export type SymbolRow = OutputGraphNodeEvidence &
  Required<
    Pick<OutputGraphNodeEvidence, 'id' | 'path' | 'symbolName' | 'kind'>
  >;
/** A `witnesses` row, as the contract states it; every witness names its source (`from`). */
export type WitnessRow = NonNullable<
  Extract<GraphQueryData, { operation: 'witnesses' }>['rows']
>[number] & { from: string };
/** The witness fields foldEdges reads: an engine row, or one rebased for the viewer stream. */
export type Witness = Pick<
  WitnessRow,
  'edge' | 'from' | 'to' | 'line' | 'confidence' | 'detail' | 'test'
>;
/** The fields read of a `diagnostics` row (the contract leaves this operation's rows open). */
interface DiagnosticRow {
  code: string;
  path: string;
  line?: number;
  test?: boolean;
  message: string;
}
/** The fields read of an `unresolved` row (the contract leaves `test` out; the row is read as the engine prints it). */
// graphQuery `unresolved` rows carry no test flag: stitched calls take it from their file's role (stitch.ts).
interface UnresolvedRow {
  from: string;
  path: string;
  targetName?: string;
  line: number;
  reason: string;
  receiverType?: string;
}
type IssueRow = Record<string, unknown>;

/** A section's snapshot before its partial flag is settled: the engine's graph fields under the build's own. */
type SectionSnapshot = Snapshot & { createdAt?: string };

/** The graph rows a section streams, by kind: file nodes, import and call witnesses, and declarations. */
export interface StreamedRows {
  files: FileRow[];
  witnesses: WitnessRow[];
  symbols: SymbolRow[];
}

/** What a section streams besides its rows: a graph page's number and total, a build progress line, or its snapshot. */
export interface SectionProgress {
  page?: number;
  total?: number;
  line?: string;
  id?: string;
  ingestMs?: number;
  frozen?: boolean;
  counts?: BuiltGraph['counts'];
  linkage?: number | null;
}

/** emit(kind, rows, page): `files`, `witnesses` or `symbols` rows (StreamedRows), or `build` / `snapshot` progress. */
export type SectionEmit = (
  kind: string,
  rows: unknown[],
  page: SectionProgress
) => void;

/** Walks one batch of graph queries to their end. */
type Walk = (fields: GraphQueryQuery[]) => Promise<GraphAnswer[]>;

const RANK: Record<string, number> = { high: 3, medium: 2, low: 1 };
const STREAMED: Record<string, string> = {
  find: 'files',
  witnesses: 'witnesses',
  symbols: 'symbols',
};
const group = topGroup;

// graphBuild refuses a tree that changes while it runs; the frozen copy lives under the result directory.
function freeze(dir: string, out: string) {
  mkdirSync(join(out, 'tmp'), { recursive: true });
  const dst = mkdtempSync(join(out, 'tmp', 'frozen-'));
  if (statSync(dir).isFile()) {
    execFileSync('rsync', ['-a', '--', resolve(dir), dst]);
    return join(dst, basename(dir));
  }
  execFileSync('rsync', [
    '-a',
    '--delete',
    ...SKIPPED_DIRS.flatMap(d => ['--exclude', d]),
    `${resolve(dir)}/`,
    `${dst}/`,
  ]);
  return dst;
}

// The snapshot of `section` (an absolute directory inside the workspace) and the path its queries repeat.
async function ingest(
  section: string,
  out: string,
  exclude: string[],
  onProgress: (line: string) => void
) {
  const run = async (path: string) => ({
    path,
    snap: await graphBuild(
      { path, ...(exclude.length ? { exclude: exclude } : {}) },
      { onProgress }
    ),
  });
  try {
    return { ...(await run(section)), frozen: false };
  } catch (e) {
    if (!/changed during graph analysis/.test(errorMessage(e))) throw e;
    return { ...(await run(freeze(section, out))), frozen: true };
  }
}

const gzipAsync = promisify(gzip);

/** A chunk's section result inside chunks/<id>/. */
export const CHUNK_DATA = 'data.json.gz';

// Evidence files, one gzip member per page. Pages compress on the thread pool while the walk continues (a
// synchronous gzip of every 8 MB page would stall all the section's other tool calls) and append in page order.
function evidenceWriter() {
  const chains = new Map<string, Promise<void>>();
  const queue = (file: string, text: string, fresh: boolean) => {
    const compressed = gzipAsync(text);
    compressed.catch(() => {}); // Awaited in order below; a failure rejects flush().
    const previous = chains.get(file) ?? Promise.resolve();
    chains.set(
      file,
      previous.then(async () =>
        (fresh ? writeFile : appendFile)(file, await compressed)
      )
    );
  };
  return {
    /** Replace the file with this one member. */
    write: (file: string, text: string) => queue(file, text, true),
    /** Add a member after the ones already written. */
    append: (file: string, text: string) => queue(file, text, false),
    async flush() {
      await Promise.all(chains.values());
    },
  };
}

// One edge per (from, to): every witness line, occurrence count, strongest confidence, resolutions seen, and
// `test` when every witness is test code (a test file, or an item compiled only under test).
export function foldEdges(witnesses: readonly Witness[], kind: string) {
  const by = new Map<string, GraphEdge>();
  for (const w of witnesses.filter(x => x.edge === kind)) {
    const k = `${w.from}>${w.to}`;
    const e = by.get(k) ?? {
      source: w.from,
      target: w.to,
      line: w.line,
      lines: [],
      count: 0,
      confidence: w.confidence,
      resolution: [],
      test: true,
    };
    e.count++;
    e.test &&= !!w.test;
    if (isInteger(w.line) && !e.lines.includes(w.line)) e.lines.push(w.line);
    // A line-less (null) witness compares as 0, as the comparison always did.
    if (Number(w.line) < Number(e.line)) e.line = w.line;
    if ((RANK[w.confidence] ?? 0) > (RANK[e.confidence] ?? 0))
      e.confidence = w.confidence;
    if (w.detail && !e.resolution.includes(w.detail))
      e.resolution.push(w.detail);
    by.set(k, e);
  }
  for (const e of by.values()) e.lines.sort((a, b) => a - b);
  return [...by.values()];
}

const label = (id: string, dup: Map<string, number>) =>
  (dup.get(basename(id)) ?? 0) > 1
    ? dirname(id) === '.'
      ? id
      : `${basename(dirname(id))}/${basename(id)}`
    : basename(id);

function fileNodes(found: FileRow[], imports: ImportEdge[]): FileNode[] {
  const { fanIn, fanOut } = degrees(imports);
  const dup = new Map<string, number>();
  for (const n of found)
    dup.set(basename(n.id), (dup.get(basename(n.id)) ?? 0) + 1);
  return found
    .map(n => ({
      id: n.id,
      label: label(n.id, dup),
      group: topGroup(n.id),
      language: n.language,
      roles: n.roles ?? [],
      fanIn: fanIn.get(n.id) ?? 0,
      fanOut: fanOut.get(n.id) ?? 0,
    }))
    .toSorted((a, b) => a.id.localeCompare(b.id));
}

function externalsOf(importEdges: GraphEdge[]) {
  const externals = new Map<string, Required<ExternalPackage>>();
  for (const e of importEdges.filter(x => x.target.startsWith('pkg:'))) {
    const x = externals.get(e.target) ?? {
      id: e.target,
      count: 0,
      files: [],
      testFiles: [],
    };
    if (e.test) x.testFiles.push(e.source);
    x.count++;
    x.files.push(e.source);
    externals.set(e.target, x);
  }
  return [...externals.values()].toSorted((a, b) => b.count - a.count);
}

// Declarations of every file (stitching binds calls between chunks to them); symbols are those in a call.
function symbolRows(
  symbolNodes: SymbolRow[],
  fileIds: Set<string>,
  calls: CallEdge[]
) {
  const inCalls = new Set(
    calls.flatMap(c => [c.source, c.target]).filter(id => id.includes('#'))
  );
  const declarations: Declaration[] = [];
  const symbols = new Map<string, SymbolNode>();
  for (const x of symbolNodes) {
    if (!fileIds.has(x.path)) continue;
    const d = {
      id: x.id,
      file: x.path,
      name: x.symbolName,
      kind: x.kind,
      line: x.line,
      endLine: x.endLine,
      exported: !!x.exported,
    };
    declarations.push(d);
    if (inCalls.has(x.id)) symbols.set(x.id, { ...d, calls: 0 });
  }
  // A split always has a last part.
  for (const id of inCalls)
    if (!symbols.has(id))
      symbols.set(id, {
        id,
        file: fileOf(id),
        name: id.split('#')[1].split('.').pop()!.replace(/@\d+$/, ''),
        kind: 'symbol',
        calls: 0,
        exported: false,
      });
  for (const c of calls) {
    const caller = symbols.get(c.source);
    if (caller) caller.calls++;
  }
  declarations.sort(
    (a, b) => a.file.localeCompare(b.file) || (a.line ?? 0) - (b.line ?? 0)
  );
  return { declarations, symbols: [...symbols.values()] };
}

async function batchedWalk(
  queries: GraphQueryQuery[],
  max: number,
  jobs: number,
  walk: Walk
) {
  const batches = [];
  for (let i = 0; i < queries.length; i += max)
    batches.push(queries.slice(i, i + max));
  return (await pool(batches, jobs, walk)).flat();
}

interface GapScope {
  files: readonly string[];
  crossPackages: Set<string> | null;
  jobs: number;
  maxTargets?: number;
  graphBatch: number;
}

// Call gaps of every file of the chunk: stitching binds through imports and also by a workspace crate path that no
// import names (`other_crate::config::read(..)`, stitch.ts crateCall), so any file can hold a site it binds; a
// filter by imports missed 1 of 8 `read_private_config` callers on the native workspace. The engine keeps only the
// CROSS_CHUNK_REASONS call sites, before paging, in concurrent file groups.
type UnresolvedReason = NonNullable<
  Extract<GraphQueryQuery, { operation: 'unresolved' }>['reasons']
>[number];

// The unresolved-call reasons whose rows can name another chunk's declaration by their spelling (stitch.ts
// bindCalls: a bare name, `Type::member`, a crate path). Left out: `member-unknown-receiver-in-repo` (a call on a
// receiver, `x.read()`, which no name lookup can bind without its type), `ambiguous` (the caller's own chunk
// declares the name more than once), `local-binding` (a closure or local the caller declares), the parse-recovery
// reasons and the argument reasons (passes edges, not calls).
export const CROSS_CHUNK_REASONS: UnresolvedReason[] = [
  'not-declared-in-repo',
  'external-package',
  'other',
  'type-qualified-unmatched',
  'constructor-target-unavailable',
];

// With no other chunk (`crossPackages` null) there is nothing to bind.
async function crossCallGaps(
  walk: Walk,
  { files, crossPackages, jobs, maxTargets, graphBatch }: GapScope
): Promise<UnresolvedCall[]> {
  if (!crossPackages || !files.length) return [];
  const max = maxTargets ?? (await prepareGraphRun()).maxTargets;
  const groups = [];
  for (let i = 0; i < files.length; i += max)
    groups.push(files.slice(i, i + max));
  const queries = groups.map((target): GraphQueryQuery => ({
    operation: 'unresolved',
    target,
    edges: ['calls'],
    reasons: CROSS_CHUNK_REASONS,
    limit: 1000,
  }));
  // Every answer is an `unresolved` one.
  const pages = (await batchedWalk(
    queries,
    graphBatch,
    jobs,
    walk
  )) as GraphAnswer<UnresolvedRow>[];
  return pages
    .flatMap(p => p.rows)
    .filter((u): u is UnresolvedRow & { targetName: string } =>
      Boolean(u.targetName)
    )
    .map(u => ({
      from: u.from,
      file: u.path,
      targetName: u.targetName,
      line: u.line,
      reason: u.reason,
      ...(u.receiverType && { receiverType: u.receiverType }),
    }));
}

function insightsOf(
  files: FileNode[],
  imports: ImportEdge[],
  cyc: GraphAnswer<CycleRow>,
  iss: GraphAnswer<IssueRow>
): Insights {
  const crossings = new Map<string, number>();
  for (const e of imports) {
    const k = `${group(e.source)}>${group(e.target)}`;
    if (group(e.source) !== group(e.target))
      crossings.set(k, (crossings.get(k) ?? 0) + 1);
  }
  const groups = [...crossings].map(([k, count]) => ({
    from: k.split('>')[0],
    to: k.split('>')[1],
    count,
  }));
  const runtime = cyc.rows.filter(c => c.runtime);
  return {
    hubs: files
      .toSorted((a, b) => b.fanIn - a.fanIn)
      .slice(0, 6)
      .map(({ id, fanIn, fanOut }) => ({ id, fanIn, fanOut })),
    health: iss.summary?.health ?? {},
    findings: iss.summary?.byDetector ?? {},
    issues: iss.rows,
    cycles: {
      all: cyc.rows,
      total: cyc.rows.length,
      runtime: runtime.length,
      largestRuntime: Math.max(0, ...runtime.map(c => c.size)),
      top: cyc.rows
        .slice(0, 5)
        .map(c => ({ size: c.size, runtime: c.runtime, nodes: c.nodes })),
    },
    groups,
    mutualGroups: groups
      .filter(
        c =>
          c.from < c.to && groups.some(d => d.from === c.to && d.to === c.from)
      )
      .map(c => [c.from, c.to]),
  };
}

export interface SectionRowsInput {
  counts: BuiltGraph['counts'];
  jobs: number;
  graphBatch: number;
  /** The most target files one evidence query takes; without it the evidence is read ungrouped. */
  fileGroup?: number;
  walk: Walk;
}

// A target is a source file in both evidence operations. Each group owns its native continuation chain.
export async function readSectionRows({
  counts,
  jobs,
  graphBatch,
  fileGroup,
  walk,
}: SectionRowsInput) {
  const fields = (
    [
      { operation: 'find', kind: 'file' },
      { operation: 'diagnostics' },
      { operation: 'cycles' },
      { operation: 'issues' },
      {
        operation: 'witnesses',
        edges: ['imports', 'calls'],
        confidence: 'low',
      },
      { operation: 'symbols' },
    ] satisfies GraphQueryQuery[]
  ).map((row): GraphQueryQuery => ({
    ...row,
    limit: Math.max(1, counts.nodes, counts.edges),
  }));
  const count = counts.nodesByKind?.file;
  if (!(
    count !== undefined &&
    fileGroup !== undefined &&
    count > fileGroup &&
    jobs > 1
  ))
    return {
      pages: await batchedWalk(fields, graphBatch, jobs, walk),
      groups: 1,
    };
  const overview = await batchedWalk(
    fields.slice(0, 4),
    graphBatch,
    jobs,
    walk
  );
  // The first field is `find` (kind file).
  const files = [
    ...new Set((overview[0] as GraphAnswer<FileRow>).rows.map(f => f.id)),
  ];
  const evidence = fields.slice(4);
  if (files.length !== count)
    return {
      pages: [
        ...overview,
        ...(await batchedWalk(evidence, graphBatch, jobs, walk)),
      ],
      groups: 1,
    };
  const batches: GraphQueryQuery[][] = [];
  const size = Math.min(fileGroup, Math.ceil(files.length / jobs));
  for (let i = 0; i < files.length; i += size) {
    const target = files.slice(i, i + size);
    // The evidence fields (witnesses, symbols) take a list of target files.
    const queries = evidence.map(
      row => ({ ...row, target }) as GraphQueryQuery
    );
    for (let j = 0; j < queries.length; j += graphBatch)
      batches.push(queries.slice(j, j + graphBatch));
  }
  const results = await pool(batches, jobs, walk);
  const combined = new Map(
    evidence.map(row => [row.operation, { rows: [] as unknown[] }])
  );
  for (const [index, pages] of results.entries())
    for (const [k, page] of pages.entries()) {
      // Every batch query is one of the evidence operations.
      const output = combined.get(batches[index][k].operation)!;
      for (const row of page.rows) output.rows.push(row);
    }
  return {
    pages: [...overview, ...combined.values()],
    groups: Math.ceil(files.length / size),
  };
}

/** The graph engine's limits for one section (lib.ts GraphLimits), its stream and the other chunks' packages. */
export interface SectionOptions {
  out: string;
  jobs?: number;
  exclude?: string[];
  emit?: SectionEmit;
  crossPackages?: Set<string> | null;
  graphBatch?: number;
  maxTargets?: number;
  fileGroup?: number;
}

// emit(kind, rows, page): kind is `files`, `witnesses` or `symbols` (rows keep the graph's own fields), `build` (one
// graphBuild progress line in page.line) or `snapshot`.
// crossPackages: package keys of the other chunks (null when this section is the only chunk).
export async function buildSection(
  section: string,
  {
    out,
    jobs = 8,
    exclude = [],
    emit = () => {},
    crossPackages = null,
    graphBatch = 1,
    maxTargets,
    fileGroup,
  }: SectionOptions
): Promise<SectionData> {
  // Kept: the lossless native pages behind every row (tmp/ holds only the frozen copy a build removes).
  const evidence = join(out, 'evidence');
  mkdirSync(evidence, { recursive: true });
  const t0 = Date.now();
  const { path, snap, frozen } = await ingest(section, out, exclude, line =>
    emit('build', [], { line })
  );
  const ingestMs = Date.now() - t0;
  // A published snapshot names its root.
  const queryRoot = statSync(path).isFile() ? dirname(snap.root!) : snap.root!;
  const workspaceRoot = await nativeWorkspaceRoot();
  // Evidence is kept whole and gzipped (lossless; read with `gzip -dc`): raw pages run to hundreds of MB.
  const saved = evidenceWriter();
  saved.write(join(evidence, 'ingest.json.gz'), JSON.stringify(snap));
  // Every query repeats the build's path and pins its snapshot id.
  const walk = (fields: GraphQueryQuery[]) => {
    const queries = fields.map(row => ({ ...row, path, graph: snap.graph }));
    const file = join(
      evidence,
      `${createHash('sha256').update(JSON.stringify(queries)).digest('hex')}.jsonl.gz`
    );
    let pages = 0;
    const run = async (tool: string, input: unknown) => {
      const response = await graphCall(tool, input);
      pages++;
      saved.append(file, `${JSON.stringify({ input, response })}\n`);
      return response;
    };
    return readGraphs(queries, {
      run,
      workspaceRoot,
      snapshotRoot: queryRoot,
      onPage: (page, index) => {
        const stream = STREAMED[queries[index].operation];
        if (stream && page.rows?.length)
          emit(stream, page.rows, { page: pages, total: totalOf(page) });
      },
    });
  };
  emit('snapshot', [], {
    id: snap.graph,
    ingestMs,
    frozen,
    counts: snap.counts,
    linkage: snap.linkage?.ratio,
  });
  const { pages, groups } = await readSectionRows({
    counts: snap.counts,
    jobs,
    graphBatch,
    fileGroup,
    walk,
  });
  // readSectionRows answers its fields in order: find (files), diagnostics, cycles, issues, witnesses, symbols.
  const [found, outside, cyc, iss, rel, sym] = pages as [
    GraphAnswer<FileRow>,
    GraphAnswer<DiagnosticRow>,
    GraphAnswer<CycleRow>,
    GraphAnswer<IssueRow>,
    GraphAnswer<WitnessRow>,
    GraphAnswer<SymbolRow>,
  ];
  const fileIds = new Set(found.rows.map(n => n.id));
  const importEdges = foldEdges(rel.rows, 'imports');
  const imports = importEdges.filter(
    e => fileIds.has(e.source) && fileIds.has(e.target)
  );
  const files = fileNodes(found.rows, imports);
  const externals = externalsOf(importEdges);
  const outsideRoot = outside.rows
    .filter(d => d.code === 'unresolved-internal' || d.code === 'outside-root')
    .map(d => ({
      file: d.path,
      line: d.line,
      ...(d.test && { test: true }),
      specifier: d.message.match(/"(.*)"/)?.[1] ?? null,
      message: d.message,
    }));
  const callEdges = foldEdges(rel.rows, 'calls');
  const calls = callEdges
    .filter(e => fileIds.has(fileOf(e.source)) && fileIds.has(fileOf(e.target)))
    .map((e): CallEdge => ({ ...e, via: 'graph' }));
  const unresolvedCalls = await crossCallGaps(walk, {
    files: files.map(f => f.id),
    crossPackages,
    jobs,
    graphBatch,
    maxTargets,
  });
  const { declarations, symbols } = symbolRows(sym.rows, fileIds, calls);
  const linked = new Set(imports.flatMap(e => [e.source, e.target]));
  const partialReasons = found.graph?.partialReasons ?? [];
  const snapshot: SectionSnapshot = {
    ...found.graph,
    id: snap.graph,
    root: resolve(path),
    queryRoot,
    counts: snap.counts,
    linkage: snap.linkage?.ratio,
    isPartial: found.graph?.isPartial === true || partialReasons.length > 0,
  };
  const data: SectionData = {
    section: resolve(section),
    evidence: 'evidence',
    groupWalks: Object.fromEntries(
      (
        [
          ['witnesses', rel],
          ['symbols', sym],
        ] as const
      ).map(([op, w]) => [
        op,
        {
          groups,
          grouped: groups > 1,
          rows: w.rows.length,
          total: w.rows.length,
        },
      ])
    ),
    coverage: {
      isPartial: snapshot.isPartial,
      partialReasons,
      files: files.length,
      isolated: files.filter(n => !linked.has(n.id)).length,
      imports: imports.length,
      calls: calls.length,
      callsOutside: callEdges.length - calls.length,
      outsideRoot: outsideRoot.length,
      declarations: declarations.length,
      symbolsInCalls: symbols.length,
    },
    frozen,
    ms: { ingest: ingestMs, total: Date.now() - t0 },
    snapshot,
    files,
    imports,
    symbols,
    calls,
    fnFiles: [
      ...new Set([
        ...symbols.map(s => s.file),
        ...calls.flatMap(c => [fileOf(c.source), fileOf(c.target)]),
      ]),
    ],
    insights: insightsOf(files, imports, cyc, iss),
    externals,
    outsideRoot,
    declarations,
    unresolvedCalls,
  };
  // A chunk is done only with its evidence on disk. Its section result is internal state that stitching and refresh
  // read back: gzipped (lossless, `gzip -dc data.json.gz`), renamed into place complete.
  await saved.flush();
  const file = join(out, CHUNK_DATA);
  await writeFile(`${file}.tmp`, await gzipAsync(JSON.stringify(data)));
  await rename(`${file}.tmp`, file);
  return data;
}
