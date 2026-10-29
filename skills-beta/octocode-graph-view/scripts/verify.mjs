#!/usr/bin/env node
// Graph leads judged by independent layers (AST, usage, clasify, optional LSP) plus image read-back.
import { execFile, execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, extname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { crop, flags, octocode } from './lib.mjs';
import { analyzeHistory } from './history.mjs';
import { selectLeads } from './leads.mjs';
import { HINTS } from './hints.mjs';
import { childUsage, countUsage } from './usage.mjs';
import { edgeVerdict, fileVerdict } from './verdict.mjs';
import { resolveModel } from './model.mjs';

const USAGE = `usage: verify.mjs --data <data.json> [--model <model.json>] [--layers ast,clasify,ocr,history[,lsp]] [--leads 8] [--lsp-timeout 150] [--history-limit 0] [--questions <file>]
  Needs shot-files.png and measure-files.json beside data.json (render.mjs --shot files,functions).
  Unavailable layers are skipped and reported. lsp is opt-in: a cold language server can take minutes.
  --questions merges {detector: {key: "question"}} over the built-in file questions.`;

const t0 = Date.now();
const argv = process.argv.slice(2);
const f = flags(argv, { data: '', model: '', 'max-commit-files': 40, 'history-limit': 0, layers: 'ast,clasify,ocr,history', leads: 8, 'lsp-timeout': 150, questions: '' });
if (!f.data) {
  console.error(USAGE);
  process.exit(2);
}
const dataPath = resolve(f.data);
const out = dirname(dataPath);
const D = JSON.parse(readFileSync(dataPath, 'utf8'));
const want = new Set(f.layers.split(','));
const host = process.env.OLLAMA_HOST?.replace(/^(?!http)/, 'http://') ?? 'http://localhost:11434';
const here = dirname(fileURLToPath(import.meta.url));
const modelPath = f.model ? resolve(f.model) : join(out, 'model.json');
const model = existsSync(modelPath) ? resolveModel(D, JSON.parse(readFileSync(modelPath, 'utf8'))) : null;
const measure = (v) => JSON.parse(readFileSync(join(out, `measure-${v}.json`), 'utf8'));
const abs = (id) => join(D.snapshot.root, id);
const chunk = (xs, n) => Array.from({ length: Math.ceil(xs.length / n) }, (_, i) => xs.slice(i * n, i * n + n));

// ---- layer availability -------------------------------------------------------------------------
const layers = {};
async function probe() {
  layers.graph = 'ok';
  if (want.has('ast')) {
    try {
      const r = await octocode(['astSearch', '--json', JSON.stringify({ queries: [{ operation: 'match', path: abs(D.files[0].id), rule: { kind: 'use_declaration' } }] })]);
      layers.ast = r.results ? 'ok' : 'skipped: no astSearch result';
    } catch (e) {
      layers.ast = `skipped: ${e.message.slice(0, 80)}`;
    }
  }
  if (want.has('ocr')) {
    try {
      const tags = await (await fetch(`${host}/api/tags`)).json();
      layers.ocr = tags.models.some((m) => m.name.startsWith('glm-ocr')) ? 'ok' : 'skipped: glm-ocr not installed';
    } catch {
      layers.ocr = `skipped: Ollama unreachable at ${host}`;
    }
  }
  if (want.has('clasify')) {
    try {
      const r = await octocode(['clasify', JSON.stringify({ queries: [{ operation: 'models' }] }), '--json']);
      layers.clasify = r.results?.[0]?.data?.models?.length ? 'ok' : 'skipped: no clasify model';
    } catch (e) {
      layers.clasify = `skipped: ${e.message.slice(0, 80)}`;
    }
  }
  if (want.has('history')) {
    try {
      execFileSync('git', ['-C', D.snapshot.root, 'rev-parse', '--show-toplevel'], { stdio: 'ignore' });
      layers.history = D.frozen ? 'skipped: section was graphed from a frozen copy' : 'ok';
    } catch {
      layers.history = 'skipped: not a git checkout';
    }
  }
  layers.lsp = want.has('lsp') ? (D.frozen ? 'skipped: section was graphed from a frozen copy' : 'ok') : 'off';
}
await probe();
const on = (l) => layers[l] === 'ok';

// ---- AST layer: every graph import edge against the parsed import statements ----------------------
const IMPORT_KINDS = { '.rs': ['use_declaration'], '.ts': ['import_statement'], '.tsx': ['import_statement'], '.js': ['import_statement'], '.jsx': ['import_statement'], '.mjs': ['import_statement'], '.cjs': ['import_statement'], '.py': ['import_statement', 'import_from_statement'], '.go': ['import_declaration'], '.java': ['import_declaration'] };
const stemOf = (id) => {
  const b = basename(id, extname(id));
  return ['index', 'mod', 'lib', 'main', '__init__'].includes(b) && dirname(id) !== '.' ? basename(dirname(id)) : b;
};
const re = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const mentions = (text, stem) => new RegExp(`\\b${re(stem)}\\b`).test(text);
const statements = new Map();
const INLINE_KINDS = { '.rs': ['scoped_identifier'], '.py': ['attribute'], '.ts': ['call_expression'], '.tsx': ['call_expression'], '.js': ['call_expression'], '.jsx': ['call_expression'], '.mjs': ['call_expression'], '.cjs': ['call_expression'], '.go': ['selector_expression'] };
const forms = new Map();
const RELATIVE = /(^|[^\w])(super|self)\b|^\s*(import|from)\s+\.|from\s+['"]\.{1,2}\//;
const connections = { checked: 0, confirmed: 0, viaImport: 0, viaPath: 0, na: 0, unconfirmed: [], mutual: [] };
async function astLayer() {
  const bySource = Map.groupBy(D.imports, (e) => e.source);
  const jobs = [...bySource.keys()].filter((id) => IMPORT_KINDS[extname(id)]);
  for (const id of bySource.keys()) if (!IMPORT_KINDS[extname(id)]) connections.na += bySource.get(id).length;
  const rows = await Promise.all(chunk(jobs, 5).map(async (b) => {
    const r = await octocode(['astSearch', '--json', JSON.stringify({ queries: b.map((id) => ({ operation: 'match', path: abs(id), rule: { any: IMPORT_KINDS[extname(id)].map((kind) => ({ kind })) } })) })]);
    return r.results.map((x, i) => [b[i], x.data?.files?.[0]?.matches ?? []]);
  }));
  for (const [id, matches] of rows.flat()) statements.set(id, matches);
  const pending = [];
  for (const id of jobs) {
    for (const e of bySource.get(id)) {
      connections.checked++;
      const s = statements.get(id)?.find((m) => m.line <= e.line && e.line < m.line + m.value.split('\n').length);
      if (s && (mentions(s.value, stemOf(e.target)) || RELATIVE.test(s.value))) { connections.viaImport++; forms.set(`${e.source}>${e.target}`, 'import'); }
      else pending.push(e);
    }
  }
  // No import statement: the dependency may be an inline qualified path (crate::mod::item(...), pkg.item(...)).
  const inline = Map.groupBy(pending.filter((e) => INLINE_KINDS[extname(e.source)]), (e) => e.source);
  const found = new Map();
  await Promise.all(chunk([...inline.keys()], 5).map(async (b) => {
    const r = await octocode(['astSearch', '--json', JSON.stringify({ queries: b.map((id) => ({ operation: 'match', path: abs(id), rule: { all: [{ any: INLINE_KINDS[extname(id)].map((kind) => ({ kind })) }, { regex: `\\b(${[...new Set(inline.get(id).map((e) => re(stemOf(e.target)))), 'super', 'self'].join('|')})\\b` }] } })) })]);
    r.results.forEach((x, i) => found.set(b[i], x.data?.files?.[0]?.matches ?? []));
  }));
  for (const e of pending) {
    const at = found.get(e.source)?.filter((m) => m.line === e.line) ?? [];
    const hit = at.some((m) => mentions(m.value, stemOf(e.target))) ? 'path' : at.some((m) => RELATIVE.test(m.value)) ? 'relative-path' : null;
    if (hit) { connections.viaPath++; forms.set(`${e.source}>${e.target}`, hit); }
    else connections.unconfirmed.push({ source: e.source, target: e.target, line: e.line });
  }
  connections.confirmed = connections.viaImport + connections.viaPath;
  const seen = new Set(D.imports.map((e) => `${e.source}>${e.target}`));
  connections.mutual = D.imports.filter((e) => e.source < e.target && seen.has(`${e.target}>${e.source}`)).map((e) => [e.source, e.target]);
}
if (on('ast')) {
  try { await astLayer(); } catch (e) { layers.ast = `error: ${e.message.slice(0, 100)}`; statements.clear(); forms.clear(); }
}

// ---- graph layer: leads -----------------------------------------------------------------------------
const fileIds = new Set(D.files.map((n) => n.id));
const leads = selectLeads(D, model, Number(f.leads));

// ---- source layer: parsed statement, imported names, how they are used ----------------------------------
const KEYWORDS = new Set(['use', 'crate', 'self', 'super', 'import', 'from', 'as', 'type', 'pub', 'mod', 'export', 'default', 'const', 'let', 'require', 'static']);
function namesOf(stmt, stem) {
  const text = stmt.replace(/\/\/.*$/gm, '').replace(/\s+/g, ' ').trim();
  let tail;
  if (/^from\b/.test(text)) tail = text.slice(text.indexOf(' import ') + 8);
  else if (/\bfrom\b/.test(text)) tail = text.slice(0, text.lastIndexOf(' from '));
  else tail = text.slice(text.lastIndexOf(stem) + stem.length);
  const ids = (tail.match(/[A-Za-z_$][\w$]*/g) ?? []).filter((x) => !KEYWORDS.has(x));
  return ids.length ? [...new Set(ids)] : [stem];
}
function sourceSignals(l) {
  const stem = stemOf(l.target);
  let lines;
  try { lines = readFileSync(abs(l.file), 'utf8').split('\n'); } catch (e) { return { ast: 'n/a', textOk: false, snippet: `unreadable: ${e.code ?? e.message}` }; }
  const snippet = (lines[l.line - 1] ?? '').trim().slice(0, 160);
  const match = statements.get(l.file)?.find((m) => m.line <= l.line && l.line < m.line + m.value.split('\n').length);
  const textOk = mentions(lines.slice(Math.max(0, l.line - 3), l.line + 6).join('\n'), stem);
  const form = forms.get(`${l.file}>${l.target}`);
  const ast = form ? 'confirmed' : statements.has(l.file) ? 'absent' : 'n/a';
  if (form === 'path' || form === 'relative-path') {
    const text = lines[l.line - 1] ?? '';
    const base = form === 'path' ? re(stem) : '(?:super|self)';
    return { ast, form, textOk, snippet, statement: text.trim().slice(0, 200), names: [(text.match(new RegExp(`\\b${base}(?:::|\\.)(\\w+)`)) ?? [])[1] ?? stem], usageLines: 1, callLike: new RegExp(`\\b${base}[\\w:.<>]*\\s*[(!]`).test(text) ? 1 : 0, reexport: false };
  }
  const stmt = match?.value ?? lines.slice(l.line - 1, l.line + 3).join('\n');
  const names = namesOf(stmt, stem);
  const from = match ? match.line : l.line;
  const to = from + stmt.split('\n').length - 1;
  const own = countUsage(lines, names, from, to);
  const kids = childUsage(abs(l.file), names);
  const usageLines = own.usage + kids.usage, callLike = own.calls + kids.calls;
  return { ast, textOk, snippet, statement: stmt.trim().slice(0, 200), names: names.slice(0, 6), usageLines, callLike, viaChildren: kids.files, reexport: /^\s*(pub(\([^)]*\))?\s+use|export\b)/.test(stmt) };
}
for (const l of leads.filter((x) => x.kind === 'edge')) l.signals.source = sourceSignals(l);

// ---- clasify layer: questions generated from each lead --------------------------------------------------
function owners(file) {
  const groups = new Map();
  for (const x of D.symbols.filter((y) => y.file === file && y.kind !== 'callee')) {
    const path = x.id.split('#')[1] ?? x.name;
    const owner = path.includes('.') ? path.slice(0, path.lastIndexOf('.')).replace(/@\d+$/, '') : 'free functions';
    groups.set(owner, [...(groups.get(owner) ?? []), x.name]);
  }
  return [...groups].sort((a, b) => b[1].length - a[1].length);
}
const UNRELATED = { unrelated: 'Does this file define two or more unrelated responsibilities?' };
const fileQuestions = { 'god-file': UNRELATED, 'critical-file': UNRELATED, 'single-point-of-failure': UNRELATED, hub: UNRELATED, ...(f.questions ? JSON.parse(readFileSync(resolve(f.questions), 'utf8')) : {}) };
function questionsFor(l) {
  if (l.kind === 'edge') {
    const s = stemOf(l.target);
    return {
      typeOnly: `Is ${s} used in this file only as a type, constant, trait or callback registration, never called as a function at runtime?`,
      runtimeCall: `Does code in this file call a function or method defined in ${s} at runtime, beyond importing it?`,
    };
  }
  if (l.detector === 'misplaced-file') return { misplaced: `Is the main purpose of this file unrelated to the name of its directory "${basename(dirname(l.file))}"?` };
  const o = owners(l.file);
  if (o.length >= 2) {
    const [[a, fa], [b, fb]] = o;
    const list = (xs) => [...new Set(xs)].slice(0, 4).join(', ');
    l.signals.graph = { ...l.signals.graph, owners: o.slice(0, 3).map(([k, xs]) => `${k} (${xs.length})`) };
    return { owners: `Do ${a} (functions ${list(fa)}) and ${b} (functions ${list(fb)}) serve unrelated purposes within this file?` };
  }
  return fileQuestions[l.detector] ?? UNRELATED;
}
const noul = (r) => Object.fromEntries(Object.entries(r.answers).map(([k, v]) => [k, v.noul]));
const usage = { input_tokens: 0, output_tokens: 0 };
const render = {};
let files = null;
try { files = measure('files'); } catch { layers.layout = 'skipped: run render.mjs --shot files,functions first (no measure-files.json)'; }
const fns = files && existsSync(join(out, 'measure-functions.json')) ? measure('functions') : null;
const labelBox = new Map((files?.labels ?? []).map((b) => [b.id, b]));
const labelOf = new Map(D.files.map((n) => [n.id, n.label]));

// Image review: one crop per edge lead, around the two nodes and their labels.
const clampBox = (b) => {
  const x = Math.max(0, Math.round(b.x)), y = Math.max(0, Math.round(b.y));
  return { x, y, w: Math.min(1600 - x, Math.round(b.w)), h: Math.min(1000 - y, Math.round(b.h)) };
};
const edgeLeads = leads.filter((l) => l.kind === 'edge' && labelBox.has(l.file) && labelBox.has(l.target)).slice(0, 4).filter((l, i) => {
  const [a, b] = [labelBox.get(l.file), labelBox.get(l.target)];
  const x0 = Math.min(a.x, b.x, a.cx, b.cx) - 50, y0 = Math.min(a.y, b.y, a.cy, b.cy) - 40;
  const x1 = Math.max(a.x + a.w, b.x + b.w, a.cx, b.cx) + 50, y1 = Math.max(a.y + a.h, b.y + b.h, a.cy, b.cy) + 40;
  try { crop(join(out, 'shot-files.png'), clampBox({ x: x0, y: y0, w: x1 - x0, h: y1 - y0 }), join(out, `crop-${i + 1}.png`), 2); } catch { return false; }
  l.cropFile = `crop-${i + 1}.png`;
  return true;
});

if (on('clasify')) try {
  const textQ = (l) => ({ operation: 'evaluate', source: { tool: 'localFetch', query: { path: abs(l.file) } }, questions: Object.fromEntries(Object.entries(questionsFor(l)).map(([k, v]) => [k, { type: 'noul', instructions: v }])) });
  const cropQ = (l) => ({
    operation: 'evaluate',
    context: 'Detail of a rendered import graph: circles are source files with their names, arrows are imports.',
    content: 'Image crop shows two files and the line between them.',
    assets: { crop: { source: { tool: 'localFetch', query: { path: relative(process.cwd(), join(out, l.cropFile)) } } } },
    questions: {
      drawn: { type: 'noul', instructions: `In image crop, is there a visible line connecting the node labelled "${labelOf.get(l.file)}" and the node labelled "${labelOf.get(l.target)}"?` },
      readable: { type: 'noul', instructions: `In image crop, are the labels "${labelOf.get(l.file)}" and "${labelOf.get(l.target)}" both fully readable and not overlapped by other text?` },
    },
  });
  const queries = [...leads.map(textQ), ...edgeLeads.map(cropQ)];
  const rows = (await Promise.all(chunk(queries, 5).map((b) => octocode(['clasify', '--json', JSON.stringify({ queries: b })])))).flatMap((r) => r.results).map((r) => r.data ?? { error: r.error });
  const bad = rows.find((r) => r.error);
  if (bad) throw new Error(`clasify: ${JSON.stringify(bad.error)}`);
  leads.forEach((l, i) => { l.signals.clasify = noul(rows[i]); });
  edgeLeads.forEach((l, i) => { l.signals.image = noul(rows[leads.length + i]); });
  rows.forEach((r) => { usage.input_tokens += r.usage.input_tokens; usage.output_tokens += r.usage.output_tokens; });
  render.model = rows[0]?.model;
} catch (e) { layers.clasify = `error: ${e.message.slice(0, 100)}`; }

// ---- lsp layer (opt-in): references of the busiest exported symbol in a lead file --------------------------
if (on('lsp')) {
  const timeout = Number(f['lsp-timeout']) * 1000;
  for (const l of leads.filter((x) => x.kind === 'file' && D.symbols.some((y) => y.file === x.file && y.line)).slice(0, 2)) {
    const own = D.symbols.filter((x) => x.file === l.file && x.line);
    const s = own.filter((x) => x.exported).sort((a, b) => b.calls - a.calls)[0] ?? own[0];
    const q = JSON.stringify({ queries: [{ operation: 'references', path: abs(l.file), symbolName: s.name.split('.').pop(), line: s.line, view: 'files' }] });
    let r;
    for (let attempt = 0; attempt < 2 && !r?.ok; attempt++) {
      try {
        r = { ok: true, data: (await octocode(['lspSearch', '--json', q], { timeout })).results[0].data };
        if (r.data.errorCode === 'timeout') r.ok = false;
      } catch (e) { r = { ok: false, data: { reason: e.message.slice(0, 120) } }; }
    }
    l.signals.lsp = r.ok ? { status: r.data.kind === 'empty' ? 'empty' : 'ok', symbol: s.name, note: r.data.reason ?? `${r.data.files?.length ?? r.data.total ?? '?'} referencing files` } : { status: 'error', symbol: s.name, note: r.data.reason ?? r.data.error ?? 'timeout' };
  }
}

// ---- history layer: churn, hotspots and co-change from one git log over the section ------------------------------
const MAX_COMMIT_FILES = Number(f['max-commit-files']);
let history = null;
if (on('history')) {
  try {
    const log = execFileSync('git', ['-C', D.snapshot.root, 'log', ...(Number(f['history-limit']) ? ['-n', String(f['history-limit'])] : []), '--relative', '--name-only', '--no-renames', '--format=@%h|%an|%ad', '--date=short', '--', '.'], { encoding: 'utf8', maxBuffer: 256 << 20 });
    history = analyzeHistory(log, D, { maxCommitFiles: MAX_COMMIT_FILES, limit: Number(f['history-limit']) });
    for (const l of leads) { const h = history.files[l.file]; if (h) l.signals.history = h; }
    // Remote history (PRs and commits on GitHub) only helps when the remote carries the path.
    const remote = (() => { try { return execFileSync('git', ['-C', D.snapshot.root, 'remote', 'get-url', 'origin'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim().match(/github\.com[:/]([^/]+)\/([^/.]+)/); } catch { return null; } })();
    if (remote) {
      const top = execFileSync('git', ['-C', D.snapshot.root, 'rev-parse', '--show-prefix'], { encoding: 'utf8' }).trim();
      history.remote = [];
      for (const h of history.hotspots.slice(0, 2)) {
        try {
          const r = await octocode(['ghSearchHistory', '--json', JSON.stringify({ queries: [{ operation: 'commit', owner: remote[1], repo: remote[2], pathPrefix: `${top}${h.id}` }] })]);
          { const cs = r.results[0].data.commits ?? []; history.remote.push({ id: h.id, commits: cs.length, latest: cs[0] ? `${cs[0].date?.slice(0, 10)} ${cs[0].messageHeadline ?? ''}`.trim() : undefined }); }
        } catch (e) { history.remote.push({ id: h.id, error: e.message.slice(0, 80) }); }
      }
    }
  } catch (e) {
    layers.history = `error: ${e.message.slice(0, 100)}`;
  }
}

// ---- synthesis: one verdict per lead ---------------------------------------------------------------------------
const lineCount = (id) => { try { return readFileSync(abs(id), 'utf8').split('\n').length; } catch { return 0; } };
for (const l of leads) {
  if (l.kind === 'edge') {
    const r = edgeVerdict(l.signals.source, l.signals.clasify);
    l.signals.votes = r.votes;
    Object.assign(l, { verdict: r.verdict, why: r.why });
  } else {
    l.signals.structure = { lines: lineCount(l.file), owners: owners(l.file).length };
    Object.assign(l, fileVerdict(l.signals.clasify, l.signals.structure, l.detector));
  }
}

// ---- render read-back: DOM layout facts, OCR with crop retry -------------------------------------------------------
const layoutOf = (m) => ({ labels: m.labels.length, overlaps: m.labelOverlaps.length, labelOnNode: m.labelOnNode.length, clipped: m.clipped.length, hidden: m.hidden ?? 0, worst: [...m.labelOverlaps].sort((a, b) => b.area - a.area).slice(0, 5) });
if (files) render.layout = { files: layoutOf(files), ...(fns ? { functions: layoutOf(fns) } : {}) };
const runOcr = (image, labels) => new Promise((res) => {
  const exp = join(mkdtempSync(join(tmpdir(), 'gv-')), 'expected.json');
  writeFileSync(exp, JSON.stringify({ labels }));
  execFile(process.execPath, [join(here, 'ocr.mjs'), image, '--expect', exp, '--max-tokens', '600'], { maxBuffer: 64 << 20 }, (err, stdout, stderr) => {
    if (err && err.code !== 1) return res({ error: (stderr || err.message).trim().slice(0, 200) });
    try { res(JSON.parse(stdout)); } catch (e) { res({ error: `ocr output: ${e.message.slice(0, 160)}` }); }
  });
});
if (on('ocr') && !files) layers.ocr = 'skipped: no measure-files.json or shot-files.png (run render.mjs --shot files,functions)';
if (on('ocr')) try {
  const expected = [...D.files.map((n) => n.label), ...D.legends.files];
  const whole = await runOcr(join(out, 'shot-files.png'), expected);
  if (whole.error) {
    layers.ocr = `error: ${whole.error}`;
  } else {
    const recovered = [], stillMissing = [];
    for (const label of whole.missing) {
      const box = files.labels.find((b) => b.label === label);
      if (!box) { stillMissing.push(label); continue; }
      const dst = join(mkdtempSync(join(tmpdir(), 'gv-')), 'crop.png');
      crop(join(out, 'shot-files.png'), clampBox({ x: box.x - 30, y: box.y - 20, w: box.w + 60, h: box.h + 40 }), dst, 3);
      const r = await runOcr(dst, [label]);
      (r.found?.length ? recovered : stillMissing).push(label);
    }
    render.ocr = { model: 'glm-ocr', ms: whole.ms, expected: expected.length, readWhole: whole.found.length, recovered, stillMissing };
  }
} catch (e) { layers.ocr = `error: ${e.message.slice(0, 100)}`; }

const conn = layers.ast === 'ok' ? connections : null;
const hints = Object.entries(layers).filter(([k, v]) => /^(skipped|error)/.test(v) && HINTS[k]).map(([k]) => ({ layer: k, enable: HINTS[k] }));
const ms = Date.now() - t0;
writeFileSync(join(out, 'verify.json'), JSON.stringify({ ms, hints, layers, connections: conn, history, leads, render, usage }));
const tally = leads.reduce((a, l) => ({ ...a, [l.verdict]: (a[l.verdict] ?? 0) + 1 }), {});
console.log(JSON.stringify({
  ms,
  layers,
  hints: hints.length ? hints : undefined,
  connections: conn && { checked: conn.checked, confirmed: conn.confirmed, viaImport: conn.viaImport, viaPath: conn.viaPath, unconfirmed: conn.unconfirmed.length, na: conn.na, mutual: conn.mutual.length },
  history: history && { ...history.quality, remote: history.remote, hotspots: history.hotspots.map((h) => `${h.id.split('/').pop()} ${h.commits}c x ${h.fanIn}in`), coChange: history.coChange.length },
  leads: leads.length,
  verdicts: tally,
  layout: render.layout && Object.fromEntries(Object.entries(render.layout).map(([k, v]) => [k, `${v.overlaps} overlaps, ${v.labelOnNode} on node, ${v.clipped} clipped, ${v.hidden} hidden`])),
  ocr: render.ocr && { whole: render.ocr.readWhole, recovered: render.ocr.recovered, stillMissing: render.ocr.stillMissing },
  clasifyTokens: usage,
}));
