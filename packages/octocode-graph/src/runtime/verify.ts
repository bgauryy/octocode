// Every graph node checked against its file on disk: the file exists, every declaration and edge line falls
// inside it, and it has not changed since its chunk's snapshot (graphQuery `stale`). Results stream in batches.
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { readGraphs } from './graph-pages.ts';
import type { GraphAnswer, GraphRun } from './graph-pages.ts';
import { errorMessage, isInteger, nativeWorkspaceRoot, pool } from './lib.ts';
import type { GraphQueryQuery } from '../contract.generated.js';
import type {
  Chunk,
  Emit,
  FileCheckStatus,
  FileNode,
  GraphData,
  SectionData,
  Verification,
} from './types.ts';
import { fileOf, prefixOf } from '../ids.ts';

/** A chunk's freshness: its files changed since the snapshot, and why freshness is unverified (empty when it is). */
export interface Freshness {
  files: { file: string; status: string }[];
  reason: string;
}

/** The fields read of a `stale` row: the changed file and how it changed. */
interface StaleRow {
  path?: unknown;
  id?: unknown;
  status?: string;
  reason?: string;
  change?: string;
}

/** One file's check, as the `verify` event streams it. */
interface FileCheckRow {
  id: string;
  path?: string;
  status: FileCheckStatus;
  lines?: number;
  bytes?: number;
  problems: string[];
}

const BATCH = 100;

// Each query targets its own pinned snapshot. Its returned workspace paths are relative
// to that snapshot's directory even when the original graph input was a single file.
export async function chunkFreshness(
  chunks: readonly Pick<Chunk, 'id' | 'path'>[],
  datas: ReadonlyMap<string, SectionData>,
  {
    jobs,
    graphBatch,
    run,
  }: { jobs: number; graphBatch: number; run?: GraphRun }
) {
  const results = new Map<string, Freshness>();
  const active: Pick<Chunk, 'id' | 'path'>[] = [];
  for (const c of chunks) {
    const data = datas.get(c.id);
    if (!data) continue;
    if (data.frozen)
      results.set(c.id, {
        files: [],
        reason:
          'built from a frozen copy, whose snapshot cannot see later edits',
      });
    else active.push(c);
  }
  const batches: Pick<Chunk, 'id' | 'path'>[][] = [];
  for (let i = 0; i < active.length; i += graphBatch)
    batches.push(active.slice(i, i + graphBatch));
  async function check(batch: Pick<Chunk, 'id' | 'path'>[]) {
    // Active chunks have data.
    const snapshots = batch.map(c => datas.get(c.id)!.snapshot);
    const queries = snapshots.map((s): GraphQueryQuery => ({
      operation: 'stale',
      path: s.root,
      graph: s.id,
    }));
    try {
      // Every answer is a `stale` one.
      // Answers are re-rooted from the engine's workspace root, never from this process's directory.
      const answers = (await readGraphs(queries, {
        run,
        snapshotRoot: snapshots.map(s => s.queryRoot),
        workspaceRoot: await nativeWorkspaceRoot(),
      })) as GraphAnswer<StaleRow>[];
      batch.forEach((c, i) => {
        const files = answers[i].rows.flatMap(row => {
          const path = row.path ?? row.id;
          return typeof path === 'string'
            ? [
                {
                  file: prefixOf(c.path) + path,
                  status: row.status ?? row.reason ?? row.change ?? 'changed',
                },
              ]
            : [];
        });
        const reason =
          answers[i].freshness?.analysisCurrent === false
            ? 'the graph analysis changed since the snapshot'
            : '';
        results.set(c.id, { files, reason });
      });
    } catch (error) {
      // A malformed page invalidates this batch. Isolate each snapshot so the others remain verifiable.
      if (batch.length > 1) {
        for (const c of batch) await check([c]);
      } else
        results.set(batch[0].id, {
          files: [],
          reason: `stale check failed: ${errorMessage(error)}`,
        });
    }
  }
  await pool(batches, jobs, check);
  return results;
}

// Lines each file must contain: declaration spans and the witness lines of edges leaving it.
function claimedLines(D: GraphData) {
  const claims = new Map<string, { line: number; what: string }[]>();
  const add = (file: string, line: number | null | undefined, what: string) => {
    if (!isInteger(line)) return;
    const list = claims.get(file) ?? [];
    list.push({ line, what });
    claims.set(file, list);
  };
  for (const s of D.symbols) {
    add(s.file, s.line, s.id);
    add(s.file, s.endLine, s.id);
  }
  for (const e of [...D.imports, ...D.calls])
    for (const l of e.lines?.length ? e.lines : [e.line])
      add(fileOf(e.source), l, `${e.source} > ${e.target}`);
  return claims;
}

interface CheckContext {
  root: string;
  stale: Map<string, string>;
  claims: Map<string, { line: number; what: string }[]>;
  freshness: ReadonlyMap<string | undefined, Freshness>;
}

async function checkFile(
  f: FileNode,
  { root, stale, claims, freshness }: CheckContext
): Promise<FileCheckRow> {
  const path = f.kind === 'package' ? f.manifest : f.id;
  if (!path)
    return {
      id: f.id,
      status: 'missing',
      problems: ['no manifest file found for this package'],
    };
  let text;
  try {
    text = await readFile(join(root, path), 'utf8');
  } catch (error) {
    const code =
      error instanceof Error &&
      'code' in error &&
      typeof error.code === 'string'
        ? error.code
        : undefined;
    return {
      id: f.id,
      path,
      status: 'missing',
      problems: [code ?? errorMessage(error)],
    };
  }
  const lines =
    text === '' ? 0 : text.split('\n').length - (text.endsWith('\n') ? 1 : 0);
  const outside = (claims.get(f.id) ?? []).filter(
    c => c.line < 1 || c.line > Math.max(lines, 1)
  );
  const problems = outside.map(
    c => `line ${c.line} of ${c.what} is outside the file (${lines} lines)`
  );
  const reason = freshness.get(f.chunk)?.reason;
  if (reason) problems.push(reason);
  const status = stale.has(f.id)
    ? 'stale'
    : outside.length
      ? 'lines'
      : reason
        ? 'unverified'
        : 'ok';
  if (stale.has(f.id))
    problems.unshift(
      `changed since the snapshot (${stale.get(f.id)}): rebuild before trusting its edges`
    );
  return {
    id: f.id,
    path,
    status,
    lines,
    bytes: Buffer.byteLength(text),
    problems,
  };
}

export interface VerifyOptions {
  root: string;
  chunks: readonly Pick<Chunk, 'id' | 'path'>[];
  datas: ReadonlyMap<string, SectionData>;
  emit: Emit;
  jobs?: number;
  graphBatch?: number;
}

export async function verifyPhase(
  D: GraphData,
  { root, chunks, datas, emit, jobs = 8, graphBatch = 1 }: VerifyOptions
) {
  const freshness = await chunkFreshness(chunks, datas, { jobs, graphBatch });
  const stale = new Map(
    [...freshness.values()].flatMap(r =>
      r.files.map((f): [string, string] => [f.file, f.status])
    )
  );
  const failures = [...freshness]
    .filter(([, r]) => r.reason)
    .map(([id, r]) => `${id}: ${r.reason}`);
  const ctx = { root, stale, claims: claimedLines(D), freshness };
  const files = D.files;
  const byId = new Map(files.map(f => [f.id, f]));
  const summary: Verification = {
    checked: 0,
    ok: 0,
    stale: 0,
    lines: 0,
    missing: 0,
    unverified: 0,
    staleCheck: failures.length ? failures.join(' · ') : 'ok',
  };
  const batches: FileNode[][] = [];
  for (let i = 0; i < files.length; i += BATCH)
    batches.push(files.slice(i, i + BATCH));
  for (const batch of batches) {
    const rows = await pool(batch, jobs, f => checkFile(f, ctx));
    for (const r of rows) {
      summary.checked++;
      summary[r.status]++;
      // Every row checks one of D.files.
      byId.get(r.id)!.check = {
        status: r.status,
        lines: r.lines,
        bytes: r.bytes,
        problems: r.problems,
      };
    }
    emit({ type: 'verify', rows, summary: { ...summary } });
    emit({
      type: 'progress',
      phase: 'verify',
      done: summary.checked,
      total: files.length,
    });
  }
  D.verification = summary;
  return summary;
}
