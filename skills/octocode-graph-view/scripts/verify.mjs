// Every graph node checked against its file on disk: the file exists, every declaration and edge line falls
// inside it, and it has not changed since its chunk's snapshot (graphQuery `stale`). Results stream in batches.
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { readGraph } from './graph-pages.mjs';
import { pool } from './lib.mjs';

const fileOf = (id) => id.split('#')[0];
const prefixOf = (path) => (path === '.' ? '' : `${path}/`);
const BATCH = 100;

// Root-relative files each built chunk's snapshot reports as changed or added since ingest.
async function staleFiles(chunks, datas, jobs) {
  const stale = new Map();
  const built = chunks.filter((c) => datas.has(c.id));
  await pool(built, jobs, async (c) => {
    const { id, root } = datas.get(c.id).snapshot;
    const r = await readGraph({ operation: 'stale', path: root, graph: id, limit: 1000 });
    for (const row of r.rows) {
      const path = row.file ?? row.path ?? row.id;
      if (typeof path === 'string') stale.set(prefixOf(c.path) + path, row.status ?? row.reason ?? row.change ?? 'changed');
    }
  });
  return stale;
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
  for (const e of [...D.imports, ...D.calls]) for (const l of e.lines ?? []) add(fileOf(e.source), l, `${e.source} > ${e.target}`);
  return claims;
}

async function checkFile(f, { root, stale, claims }) {
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
  const problems = outside.slice(0, 20).map((c) => `line ${c.line} of ${c.what} is outside the file (${lines} lines)`);
  if (outside.length > 20) problems.push(`${outside.length - 20} more lines outside the file`);
  const status = stale.has(f.id) ? 'stale' : outside.length ? 'lines' : 'ok';
  if (stale.has(f.id)) problems.unshift(`changed since the snapshot (${stale.get(f.id)}): rebuild before trusting its edges`);
  return { id: f.id, path, status, lines, bytes: Buffer.byteLength(text), problems };
}

export async function verifyPhase(D, { root, chunks, datas, emit, jobs = 8 }) {
  let staleError = '';
  const stale = await staleFiles(chunks, datas, jobs).catch((error) => { staleError = String(error.message).split('\n')[0]; return new Map(); });
  const ctx = { root, stale, claims: claimedLines(D) };
  const files = D.files;
  const byId = new Map(files.map((f) => [f.id, f]));
  const summary = { checked: 0, ok: 0, stale: 0, lines: 0, missing: 0, staleCheck: staleError ? `failed: ${staleError}` : 'ok' };
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
