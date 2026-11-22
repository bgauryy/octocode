// Every graph node checked against its file on disk: the file exists, every declaration and edge line falls
// inside it, and it has not changed since its chunk's snapshot (graphQuery `stale`). Results stream in batches.
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { readGraphs } from './graph-pages.mjs';
import { pool } from './lib.mjs';

const fileOf = (id) => id.split('#')[0];
const prefixOf = (path) => (path === '.' ? '' : `${path}/`);
const BATCH = 100;

// Each query targets its own pinned snapshot. Its returned workspace paths are relative
// to that snapshot's directory even when the original graph input was a single file.
export async function chunkFreshness(chunks, datas, { jobs, graphBatch, run }) {
  const results = new Map();
  const active = [];
  for (const c of chunks) {
    const data = datas.get(c.id);
    if (!data) continue;
    if (data.frozen) results.set(c.id, { files: [], reason: 'built from a frozen copy, whose snapshot cannot see later edits' });
    else active.push(c);
  }
  const batches = [];
  for (let i = 0; i < active.length; i += graphBatch) batches.push(active.slice(i, i + graphBatch));
  async function check(batch) {
    const snapshots = batch.map(c => datas.get(c.id).snapshot);
    const queries = snapshots.map(s => ({ operation: 'stale', path: s.root, graph: s.id }));
    try {
      const answers = await readGraphs(queries, { run, snapshotRoot: snapshots.map(s => s.queryRoot) });
      batch.forEach((c, i) => {
        const files = answers[i].rows.flatMap(row => {
          const path = row.path ?? row.id;
          return typeof path === 'string' ? [{ file:prefixOf(c.path) + path, status:row.status ?? row.reason ?? row.change ?? 'changed' }] : [];
        });
        const reason = answers[i].freshness?.analysisCurrent === false ? 'the graph analysis changed since the snapshot' : '';
        results.set(c.id, { files, reason });
      });
    } catch (error) {
      // A malformed page invalidates this batch. Isolate each snapshot so the others remain verifiable.
      if (batch.length > 1) {
        for (const c of batch) await check([c]);
      } else results.set(batch[0].id, { files: [], reason: `stale check failed: ${error.message}` });
    }
  }
  await pool(batches, jobs, check);
  return results;
}

// Lines each file must contain: declaration spans and the witness lines of edges leaving it.
function claimedLines(D) {
  const claims = new Map();
  const add = (file, line, what) => {
    if (!Number.isInteger(line)) return;
    const list = claims.get(file) ?? [];
    list.push({ line, what });
    claims.set(file, list);
  };
  for (const s of D.symbols) { add(s.file, s.line, s.id); add(s.file, s.endLine, s.id); }
  for (const e of [...D.imports, ...D.calls]) for (const l of e.lines?.length ? e.lines : [e.line]) add(fileOf(e.source), l, `${e.source} > ${e.target}`);
  return claims;
}

async function checkFile(f, { root, stale, claims, freshness }) {
  const path = f.kind === 'package' ? f.manifest : f.id;
  if (!path) return { id: f.id, status: 'missing', problems: ['no manifest file found for this package'] };
  let text;
  try {
    text = await readFile(join(root, path), 'utf8');
  } catch (error) {
    return { id: f.id, path, status: 'missing', problems: [error.code ?? error.message] };
  }
  const lines = text === '' ? 0 : text.split('\n').length - (text.endsWith('\n') ? 1 : 0);
  const outside = (claims.get(f.id) ?? []).filter((c) => c.line < 1 || c.line > Math.max(lines, 1));
  const problems = outside.map((c) => `line ${c.line} of ${c.what} is outside the file (${lines} lines)`);
  const reason = freshness.get(f.chunk)?.reason;
  if (reason) problems.push(reason);
  const status = stale.has(f.id) ? 'stale' : outside.length ? 'lines' : reason ? 'unverified' : 'ok';
  if (stale.has(f.id)) problems.unshift(`changed since the snapshot (${stale.get(f.id)}): rebuild before trusting its edges`);
  return { id: f.id, path, status, lines, bytes: Buffer.byteLength(text), problems };
}

export async function verifyPhase(D, { root, chunks, datas, emit, jobs = 8, graphBatch = 1 }) {
  const freshness = await chunkFreshness(chunks, datas, { jobs, graphBatch });
  const stale = new Map([...freshness.values()].flatMap(r => r.files.map(f => [f.file, f.status])));
  const failures = [...freshness].filter(([, r]) => r.reason).map(([id, r]) => `${id}: ${r.reason}`);
  const ctx = { root, stale, claims: claimedLines(D), freshness };
  const files = D.files;
  const byId = new Map(files.map((f) => [f.id, f]));
  const summary = { checked: 0, ok: 0, stale: 0, lines: 0, missing: 0, unverified: 0, staleCheck: failures.length ? failures.join(' · ') : 'ok' };
  const batches = [];
  for (let i = 0; i < files.length; i += BATCH) batches.push(files.slice(i, i + BATCH));
  for (const batch of batches) {
    const rows = await pool(batch, jobs, (f) => checkFile(f, ctx));
    for (const r of rows) {
      summary.checked++;
      summary[r.status]++;
      byId.get(r.id).check = { status: r.status, lines: r.lines, bytes: r.bytes, problems: r.problems };
    }
    emit({ type: 'verify', rows, summary: { ...summary } });
    emit({ type: 'progress', phase: 'verify', done: summary.checked, total: files.length });
  }
  D.verification = summary;
  return summary;
}
