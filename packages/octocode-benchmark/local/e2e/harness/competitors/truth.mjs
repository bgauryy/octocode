// The truth check both sides of a competitor task are judged by: a task's
// expected hits, ranges, files, counts and text against the line/file evidence
// a side collected (`emptyEvidence()` shape).
import fs from 'node:fs';
import path from 'node:path';
import { ROOT } from '../paths.mjs';

/** Evidence a side collects: line pairs, file names (and listed rows in order), and all text. */
export const emptyEvidence = () => ({ pairs: [], files: new Set(), fileRows: [], pairFiles: new Set(), text: '' });

const norm = f => String(f).replace(/^file:\/\//, '').replace(/^(\.\/|[ab]\/)/, '');
const matchFile = (evidence, truth) => { const e = norm(evidence); return e === truth || e.endsWith(`/${truth}`); };
const pairKey = p => `${norm(p.file)}:${p.line}`;
const where = t => `${path.basename(t.file)}:${t.line}`;

/** The task's truth; a `live` truth is the first line matching its regex today (null when absent). */
export function liveTruth(task) {
  const live = task.truth.live;
  if (!live) return task.truth;
  const full = path.join(ROOT, live.file);
  if (!fs.existsSync(full)) return null;
  const index = fs.readFileSync(full, 'utf8').split('\n').findIndex(l => new RegExp(live.regex).test(l));
  return index < 0 ? null : { hits: [{ file: live.file, line: index + 1 }] };
}

function lineFails(truth, ev, has) {
  const fails = [];
  for (const h of truth.hits ?? []) if (!has(h.file, h.line)) fails.push(`missing ${h.file}:${h.line}`);
  for (const r of [truth.range, ...(truth.ranges ?? [])].filter(Boolean)) {
    const missing = [];
    for (let l = r.start; l <= r.end; l += 1) if (!has(r.file, l)) missing.push(l);
    if (missing.length) fails.push(`range ${r.file}:${r.start}-${r.end} missing ${missing.length} lines (${missing.slice(0, 5).join(',')})`);
  }
  if (truth.exactHits) {
    const missing = truth.exactHits.filter(t => !has(t.file, t.line));
    const extra = [...new Set(ev.pairs.filter(p => !truth.exactHits.some(t => t.line === p.line && matchFile(p.file, t.file))).map(where))];
    if (missing.length) fails.push(`exactHits missing ${missing.length}: ${missing.slice(0, 4).map(where).join(',')}`);
    if (extra.length) fails.push(`exactHits extra ${extra.length}: ${extra.slice(0, 4).join(',')}`);
  }
  return fails;
}

function fileFails(truth, ev) {
  const fails = [];
  const allFiles = [...ev.files, ...ev.pairFiles, ...ev.pairs.map(p => p.file)];
  for (const f of truth.files ?? []) if (!allFiles.some(e => matchFile(e, f))) fails.push(`missing file ${f}`);
  if (truth.exactFiles) {
    const files = [...ev.files];
    const missing = truth.exactFiles.filter(t => !files.some(e => matchFile(e, t)));
    const extra = files.filter(e => !truth.exactFiles.some(t => matchFile(e, t)));
    if (missing.length) fails.push(`exactFiles missing ${missing.length}/${truth.exactFiles.length}: ${missing.slice(0, 3).join(',')}`);
    if (extra.length) fails.push(`exactFiles extra ${extra.length}: ${extra.slice(0, 3).join(',')}`);
  }
  return fails;
}

function countFails(truth, ev) {
  const fails = [];
  if (truth.count != null) {
    const n = new Set(ev.pairs.map(pairKey)).size;
    if (n !== truth.count) fails.push(`count ${n} != ${truth.count}`);
  }
  if (truth.fileCount != null) {
    const n = new Set([...ev.files].map(norm)).size;
    if (n !== truth.fileCount) fails.push(`fileCount ${n} != ${truth.fileCount}`);
  }
  // Every row seen exactly once across all pages: a continuation that skips
  // or repeats rows is a correctness defect, not an efficiency loss.
  if (truth.exactOnce) {
    const keys = truth.exactOnce === 'files' ? ev.fileRows.map(norm) : ev.pairs.map(pairKey);
    const counts = new Map();
    for (const k of keys) counts.set(k, (counts.get(k) ?? 0) + 1);
    const repeated = [...counts].filter(([, n]) => n > 1).map(([k, n]) => `${k}×${n}`);
    if (repeated.length) fails.push(`exactOnce repeated ${repeated.length}: ${repeated.slice(0, 4).join(',')}`);
  }
  return fails;
}

/** Judge one side's evidence: {ok, fails}. */
export function evaluate(truth, ev) {
  const has = (file, line) => ev.pairs.some(p => p.line === line && matchFile(p.file, file));
  const fails = [...lineFails(truth, ev, has), ...fileFails(truth, ev), ...countFails(truth, ev)];
  for (const re of truth.text ?? []) if (!new RegExp(re).test(ev.text)) fails.push(`text /${re}/ not found`);
  return { ok: fails.length === 0, fails };
}

/** Coverage facts the byte gate pairs with: unique line pairs and unique files seen. */
export function evidenceCounts(ev) {
  return {
    pairs: new Set(ev.pairs.map(pairKey)).size,
    files: new Set([...ev.files, ...ev.pairFiles, ...ev.pairs.map(p => p.file)].map(norm)).size,
  };
}
