// Deterministic competitor harness (beat-rg-gh PLAN S5): frozen tasks run as
// an octocode recipe (MCP via the shared client; CLI for CLI-only tools) and
// as the shell recipe an expert agent would run (rg/sed/git/gh/ast-grep/npm).
// Both sides are judged by the same truth check; calls, model-visible bytes
// (structuredContent JSON / stdout+stderr) and wall ms are recorded per side.
// Exit is non-zero only when an octocode answer is wrong; efficiency losses
// are reported, not failed.
//
// Usage: node harness/competitors.mjs [--only=L01,G03] [--kind=local|github|clasify] [--save-as=<results name>]
// Env:   OCTOCODE_COMPETITOR_CLASIFY=1   run the clasify tasks (paid provider calls)
//        OCTOCODE_COMPETITOR_GITHUB=0    skip GitHub/network tasks
//        OCTOCODE_COMPETITOR_BASH_TOOL_BYTES  Bash tool definition estimate (default 1200)
//        RG_BIN                          ripgrep binary (else rg on PATH, else Claude Code's embedded rg)
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { ROOT, checks, expandShared, inventoryRows, nextHints, startServer, writeResults } from './mcp-client.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const { tasks: ALL } = JSON.parse(fs.readFileSync(path.join(HERE, 'competitor-tasks.json'), 'utf8'));
const T = 'octocode-local-testing/repos';
const CLI = path.join(ROOT, 'packages/octocode/out/octocode.js');
const CT = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-competitors-'));
const BASH_TOOL_DEF_BYTES = Number(process.env.OCTOCODE_COMPETITOR_BASH_TOOL_BYTES ?? 1200);
const REASONING = 'Answer the task question.';
const arg = name => process.argv.find(a => a.startsWith(`--${name}=`))?.slice(name.length + 3);
const only = arg('only')?.split(',');
const kinds = arg('kind')?.split(',');
const RATE_LIMIT = /rate.?limit|secondary rate|abuse detection|API rate limit exceeded|HTTP 429/i;
const TRANSIENT = /MCP server|server unavailable|exited|timeout tools\/call|fingerprint|ECONNRESET|suite MCP deadline/i;

const { check, summary } = checks('competitors');

// ---------- shell side ----------
function rgPreamble() {
  if (process.env.RG_BIN) return `rg() { "${process.env.RG_BIN}" "$@"; }`;
  if (spawnSync('bash', ['-c', 'type -P rg'], { encoding: 'utf8' }).stdout.trim()) return '';
  const claude = process.env.CLAUDE_CODE_EXECPATH;
  if (claude && fs.existsSync(claude)) return `rg() { (exec -a rg "${claude}" "$@"); }`;
  throw new Error('ripgrep not found: set RG_BIN');
}
const PREAMBLE = rgPreamble();

function sh(step) {
  const started = performance.now();
  const run = spawnSync('bash', ['-c', `${PREAMBLE}\n${step.sh}`], {
    cwd: ROOT, encoding: 'utf8', maxBuffer: 256 << 20, timeout: 120_000,
    env: { ...process.env, T, CT, ROOT, GH_PAGER: '', NO_COLOR: '1' },
  });
  const ms = Math.round(performance.now() - started);
  const stdout = run.stdout ?? '';
  const stderr = run.stderr ?? '';
  return { cmd: step.sh, ms, bytes: stdout.length + stderr.length, exit: run.status, stdout, stderr };
}

/** Line/file evidence from one shell step's stdout, by the step's parse mode. */
function shellEvidence(step, out, ev) {
  const lines = out.stdout.split('\n').filter(l => l.length);
  const add = (file, line) => { ev.pairs.push({ file, line }); };
  for (const l of lines) {
    switch (step.parse) {
      case 'rg': {
        const m = /^(.+?)[:-](\d+)[:-]/.exec(l);
        if (m) { add(m[1], +m[2]); ev.pairFiles.add(m[1]); } else if (!/\s/.test(l.trim())) ev.files.add(l.trim());
        break;
      }
      case 'numbered': { const m = /^\s*(\d+)[:\t│\- ]/.exec(l); if (m) add(step.file, +m[1]); break; }
      case 'files': ev.files.add(l.trim()); break;
      case 'filecolon': { const m = /^(\S+?):\s/.exec(l); if (m) ev.files.add(m[1]); break; }
      case 'diff': { const m = /^diff --git a\/(\S+) b\//.exec(l); if (m) ev.files.add(m[1]); break; }
      case 'ghsearch': { const m = /^[^\s:]+\/[^\s:]+:([^:]+):/.exec(l); if (m) ev.files.add(m[1]); break; }
      default: break;
    }
  }
  if (step.parse === 'sed') out.stdout.split('\n').slice(0, -1).forEach((_, i) => add(step.file, step.start + i));
  ev.text += `${out.stdout}\n${out.stderr}\n`;
}

// ---------- octocode side ----------
let client;
async function restart() {
  try { client?.close(); } catch {}
  client = await startServer({ env: { OCTOCODE_BETA: 'true' } });
}

async function mcp(tool, args) {
  const started = performance.now();
  let response;
  try { response = await client.rpc('tools/call', { name: tool, arguments: args }); } catch (error) { response = { error: { message: error.message } }; }
  const ms = Math.round(performance.now() - started);
  const rawSc = response.result?.structuredContent;
  const text = response.result?.content?.filter(c => c.type === 'text').map(c => c.text).join('') ?? JSON.stringify(response.error ?? '');
  return { surface: 'mcp', tool, args, ms, bytes: rawSc ? JSON.stringify(rawSc).length : text.length, sc: expandShared(rawSc), text, isError: !!(response.error || response.result?.isError), transport: response.error?.message };
}

function cli(tool, args) {
  const started = performance.now();
  const run = spawnSync(process.execPath, [CLI, tool, JSON.stringify(args)], { cwd: ROOT, encoding: 'utf8', maxBuffer: 256 << 20, timeout: 180_000, env: { ...process.env, OCTOCODE_BETA: 'true' } });
  const ms = Math.round(performance.now() - started);
  const stdout = run.stdout ?? '';
  let sc; try { sc = JSON.parse(stdout); } catch {}
  return { surface: 'cli', tool, args, ms, bytes: stdout.length + (run.stderr ?? '').length, sc, text: stdout + (run.stderr ?? ''), isError: run.status !== 0, transport: run.error?.message };
}

const get = (obj, dotted) => dotted.split('.').reduce((o, k) => (o == null ? undefined : o[k]), obj);
function resolve(value, entries) {
  if (typeof value === 'string') {
    return value.replace(/\{\{([^}]+)\}\}/g, (_, expr) => {
      if (expr === 'T') return T;
      if (expr === 'ROOT') return ROOT;
      const [kind, rest] = [expr.slice(0, expr.indexOf(':')), expr.slice(expr.indexOf(':') + 1)];
      let found;
      if (kind === 'prev') found = get(entries.at(-1)?.sc, rest);
      if (kind === 'hint') {
        const [name, ...tail] = rest.split('.');
        const hint = entries.slice().reverse().flatMap(e => nextHints(e.sc)).find(h => h.path.endsWith(`.${name}`));
        found = hint && get(hint, tail.join('.'));
      }
      if (found == null) throw new Error(`unresolved template {{${expr}}}`);
      return String(found);
    });
  }
  if (Array.isArray(value)) return value.map(v => resolve(v, entries));
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, resolve(v, entries)]));
  return value;
}

async function runOctocode(task) {
  const goal = task.question;
  const brief = q => ({ goal, reasoning: REASONING, ...q });
  const entries = [];
  const evidenceSteps = [];
  for (const step of task.octocode) {
    const before = entries.length;
    if (step.tool) {
      const queries = step.queries ? step.queries.map(q => brief(resolve(q, entries))) : [brief(resolve(step.args, entries))];
      entries.push(await mcp(step.tool, { ...step.envelope, queries }));
    } else if (step.raw) {
      entries.push(await mcp(step.raw, brief(resolve(step.args, entries))));
    } else if (step.cli) {
      entries.push(cli(step.cli, { queries: [brief(resolve(step.args, entries))] }));
    } else if (step.follow) {
      const hint = entries.slice().reverse().flatMap(e => nextHints(e.sc)).find(h => h.path.endsWith(`.${step.follow}`));
      if (!hint) throw new Error(`no next.${step.follow} to follow`);
      entries.push(await mcp(hint.tool, { queries: [hint.query] }));
    } else if (step.walk) {
      // Ordered outer → inner: a response reached through an inner
      // continuation offers only that one or deeper (no page × matchPage fan-out).
      const order = step.walk;
      const seen = new Set();
      const queue = [];
      const enqueue = (entry, minIndex) => {
        for (const h of nextHints(entry.sc)) {
          const name = h.path.split('.').at(-1);
          const index = order.indexOf(name);
          if (index < minIndex) continue;
          const key = `${h.tool}:${JSON.stringify(h.query)}`;
          if (!seen.has(key)) { seen.add(key); queue.push({ hint: h, index }); }
        }
      };
      for (const e of entries.slice(evidenceSteps.at(-1)?.from ?? 0)) enqueue(e, 0);
      let calls = 0;
      while (queue.length && calls < (step.max ?? 40)) {
        const { hint, index } = queue.shift();
        // A row continuation runs in `queries` (with the step's call-level
        // options, e.g. an explicit responseCharLength); an envelope
        // continuation (responsePagination.next) is the whole call.
        const e = await mcp(hint.tool, hint.query.queries ? hint.query : { ...step.envelope, queries: [hint.query] });
        entries.push(e); calls += 1;
        if (e.isError) break;
        enqueue(e, index);
      }
    }
    evidenceSteps.push({ from: before, to: entries.length, unsearched: !!step.unsearched });
  }
  return { entries, evidenceSteps };
}

/** Line/file evidence from octocode structured results (continuations excluded). */
function octocodeEvidence(entry, ev, { unsearched = false } = {}) {
  const pair = (file, line) => { if (file && Number.isInteger(line)) ev.pairs.push({ file, line }); };
  const numbered = (file, content) => {
    for (const m of String(content).matchAll(/^(\d+)\t/gm)) pair(file, +m[1]);
  };
  const walk = (node, ctx, inArray) => {
    if (!node || typeof node !== 'object') return;
    if (Array.isArray(node)) { for (const item of node) walk(item, ctx, true); return; }
    let here = ctx;
    if (typeof node.path === 'string') { here = node.path; if (inArray) ev.files.add(node.path); }
    if (typeof node.file === 'string') ev.files.add(node.file);
    if (node.from?.path && Array.isArray(node.fromRanges)) { for (const r of node.fromRanges) pair(node.from.path, r.startLine); return; }
    if (node.displayRange && typeof node.path === 'string') pair(node.path, node.displayRange.startLine);
    if (typeof node.line === 'number') pair(here, node.line);
    if (typeof node.content === 'string') numbered(here, node.content);
    if (Array.isArray(node.lines) && node.lines.every(l => typeof l === 'string')) for (const l of node.lines) { const m = /^(\d+)\t/.exec(l); if (m) pair(here, +m[1]); }
    if (Array.isArray(node.byFile)) for (const f of node.byFile) for (const ref of f.refs ?? []) { const m = /^(\d+)/.exec(ref); if (m) pair(f.path, +m[1]); }
    if (typeof node.dir === 'string' && Array.isArray(node.files)) {
      // ghStructure dirs are relative to the requested path (the agent's own input).
      const base = entry.tool === 'ghStructure' ? String(entry.args?.queries?.[0]?.path ?? '').replace(/^\.?\/?$/, '') : '';
      const rel = node.dir === '.' || node.dir === '' ? '' : node.dir.replace(/\/$/, '');
      const joined = [base.replace(/\/$/, ''), rel].filter(Boolean).join('/');
      const dir = joined ? `${joined}/` : '';
      for (const name of node.files) if (typeof name === 'string') ev.files.add(dir + name);
    }
    for (const [key, child] of Object.entries(node)) {
      if (key === 'next') continue;
      if (key === 'unsearchedFiles' && Array.isArray(child)) { if (unsearched) for (const s of child) { const m = /^!\w+ (.+)$/.exec(s); if (m) ev.files.add(m[1]); } continue; }
      if (key === 'changedFiles' && Array.isArray(child) && child.some(c => typeof c === 'string' || (c && !('path' in c)))) { for (const f of inventoryRows(child)) if (f.path) ev.files.add(f.path); continue; }
      if (key === 'from' || key === 'displayRange') continue;
      walk(child, here, Array.isArray(child) ? true : false);
    }
  };
  walk(entry.sc, undefined, false);
  ev.text += `${leaves(entry.sc ?? entry.text).join('\n')}\n`;
}
function leaves(value, out = []) {
  if (value == null) return out;
  if (typeof value !== 'object') { out.push(String(value)); return out; }
  for (const child of Object.values(value)) leaves(child, out);
  return out;
}

// ---------- truth ----------
const norm = f => String(f).replace(/^file:\/\//, '').replace(/^(\.\/|[ab]\/)/, '');
const matchFile = (evidence, truth) => { const e = norm(evidence); return e === truth || e.endsWith(`/${truth}`); };
function liveTruth(task) {
  const live = task.truth.live;
  if (!live) return task.truth;
  const full = path.join(ROOT, live.file);
  if (!fs.existsSync(full)) return null;
  const index = fs.readFileSync(full, 'utf8').split('\n').findIndex(l => new RegExp(live.regex).test(l));
  return index < 0 ? null : { hits: [{ file: live.file, line: index + 1 }] };
}
function evaluate(truth, ev) {
  const fails = [];
  const has = (file, line) => ev.pairs.some(p => p.line === line && matchFile(p.file, file));
  for (const h of truth.hits ?? []) if (!has(h.file, h.line)) fails.push(`missing ${h.file}:${h.line}`);
  for (const r of [truth.range, ...(truth.ranges ?? [])].filter(Boolean)) {
    const missing = [];
    for (let l = r.start; l <= r.end; l += 1) if (!has(r.file, l)) missing.push(l);
    if (missing.length) fails.push(`range ${r.file}:${r.start}-${r.end} missing ${missing.length} lines (${missing.slice(0, 5).join(',')})`);
  }
  if (truth.exactHits) {
    const missing = truth.exactHits.filter(t => !has(t.file, t.line));
    const extra = [...new Set(ev.pairs.filter(p => !truth.exactHits.some(t => t.line === p.line && matchFile(p.file, t.file))).map(p => `${path.basename(p.file)}:${p.line}`))];
    if (missing.length) fails.push(`exactHits missing ${missing.length}: ${missing.slice(0, 4).map(t => `${path.basename(t.file)}:${t.line}`).join(',')}`);
    if (extra.length) fails.push(`exactHits extra ${extra.length}: ${extra.slice(0, 4).join(',')}`);
  }
  const allFiles = [...ev.files, ...ev.pairFiles, ...ev.pairs.map(p => p.file)];
  for (const f of truth.files ?? []) if (!allFiles.some(e => matchFile(e, f))) fails.push(`missing file ${f}`);
  if (truth.exactFiles) {
    const files = [...ev.files];
    const missing = truth.exactFiles.filter(t => !files.some(e => matchFile(e, t)));
    const extra = files.filter(e => !truth.exactFiles.some(t => matchFile(e, t)));
    if (missing.length) fails.push(`exactFiles missing ${missing.length}/${truth.exactFiles.length}: ${missing.slice(0, 3).join(',')}`);
    if (extra.length) fails.push(`exactFiles extra ${extra.length}: ${extra.slice(0, 3).join(',')}`);
  }
  if (truth.count != null) {
    const n = new Set(ev.pairs.map(p => `${norm(p.file)}:${p.line}`)).size;
    if (n !== truth.count) fails.push(`count ${n} != ${truth.count}`);
  }
  for (const re of truth.text ?? []) if (!new RegExp(re).test(ev.text)) fails.push(`text /${re}/ not found`);
  return { ok: fails.length === 0, fails };
}
const emptyEvidence = () => ({ pairs: [], files: new Set(), pairFiles: new Set(), text: '' });

// ---------- task runner ----------
let lastCodeSearch = 0;
async function paceCodeSearch() {
  const wait = lastCodeSearch + 7000 - Date.now();
  if (wait > 0) await new Promise(r => setTimeout(r, wait));
  lastCodeSearch = Date.now();
}

async function octocodeSide(task, truth) {
  for (let attempt = 0; attempt < 2; attempt += 1) {
    let run, error;
    try { run = await runOctocode(task); } catch (e) { error = e; }
    const entries = run?.entries ?? [];
    const transient = entries.find(e => (e.transport && TRANSIENT.test(e.transport)) || (e.isError && TRANSIENT.test(e.text.slice(0, 400))));
    if (transient && attempt === 0) { console.log(`RETRY [competitors] ${task.id}: ${String(transient.transport ?? transient.text).slice(0, 120)}; restarting MCP`); await restart(); continue; }
    if (entries.some(e => e.isError && RATE_LIMIT.test(e.text))) return { skipped: 'rate limited' };
    const ev = emptyEvidence();
    for (const s of run?.evidenceSteps ?? []) for (const e of entries.slice(s.from, s.to)) octocodeEvidence(e, ev, s);
    const verdict = error ? { ok: false, fails: [error.message] } : evaluate(truth, ev);
    return {
      ...verdict, calls: entries.length, bytes: entries.reduce((a, e) => a + e.bytes, 0), ms: entries.reduce((a, e) => a + e.ms, 0),
      steps: entries.map(e => ({ tool: e.tool, surface: e.surface, ms: e.ms, bytes: e.bytes, isError: e.isError })),
      errors: entries.filter(e => e.isError).map(e => e.text.slice(0, 200)),
    };
  }
}

function competitorSide(task, truth) {
  const ev = emptyEvidence();
  const steps = [];
  for (const step of task.competitor) {
    const out = sh(step);
    steps.push({ cmd: out.cmd, ms: out.ms, bytes: out.bytes, exit: out.exit });
    if (RATE_LIMIT.test(out.stderr)) return { skipped: 'rate limited' };
    shellEvidence(step, out, ev);
  }
  return { ...evaluate(truth, ev), calls: steps.length, bytes: steps.reduce((a, s) => a + s.bytes, 0), ms: steps.reduce((a, s) => a + s.ms, 0), steps };
}

async function runTask(task) {
  const truth = liveTruth(task);
  if (!truth) return { status: 'skipped', skipReason: 'live truth unavailable' };
  if (task.kind === 'clasify' && process.env.OCTOCODE_COMPETITOR_CLASIFY !== '1') return { status: 'skipped', skipReason: 'paid provider calls (set OCTOCODE_COMPETITOR_CLASIFY=1)' };
  if (task.kind === 'github' && process.env.OCTOCODE_COMPETITOR_GITHUB === '0') return { status: 'skipped', skipReason: 'GitHub disabled (OCTOCODE_COMPETITOR_GITHUB=0)' };
  // Local tasks run twice; the second (warm server, warm page cache) is recorded.
  const repeats = task.kind === 'local' ? 2 : 1;
  let octocode, competitor, coldOcto, coldComp;
  for (let i = 0; i < repeats; i += 1) {
    if (task.codeSearch) await paceCodeSearch();
    coldOcto ??= octocode; octocode = await octocodeSide(task, truth);
    if (task.codeSearch) await paceCodeSearch();
    coldComp ??= competitor; competitor = competitorSide(task, truth);
    if (octocode.skipped || competitor.skipped) return { status: 'skipped', skipReason: `rate limited (${octocode.skipped ? 'octocode' : 'competitor'})` };
  }
  if (coldOcto) octocode.coldMs = coldOcto.ms;
  if (coldComp) competitor.coldMs = coldComp.ms;
  const ratio = (a, b) => (b > 0 ? +(a / b).toFixed(2) : null);
  return { status: 'ran', truth, octocode, competitor, ratios: { calls: ratio(octocode.calls, competitor.calls), bytes: ratio(octocode.bytes, competitor.bytes), ms: ratio(octocode.ms, competitor.ms) } };
}

// ---------- fixed overhead ----------
async function fixedOverhead() {
  const plain = await startServer({ env: { OCTOCODE_BETA: 'false' } });
  const out = {
    toolsListBytes: JSON.stringify(plain.tools).length, tools: plain.tools.length,
    instructionsBytes: (plain.init?.instructions ?? '').length,
    betaToolsListBytes: JSON.stringify(client.tools).length, betaTools: client.tools.length,
    bashToolDefBytes: BASH_TOOL_DEF_BYTES,
    assumption: 'Shell side = one Bash tool definition (name, ~1 paragraph description, {command, timeout, description} input schema) ≈ BASH_TOOL_DEF_BYTES; the host re-sends tool definitions every turn on both sides, so only the difference matters. Override with OCTOCODE_COMPETITOR_BASH_TOOL_BYTES.',
  };
  plain.close();
  out.octocodeTotal = out.toolsListBytes + out.instructionsBytes;
  out.deltaVsBash = out.octocodeTotal - BASH_TOOL_DEF_BYTES;
  return out;
}

// ---------- main ----------
const median = xs => { const s = xs.filter(x => x != null && Number.isFinite(x)).sort((a, b) => a - b); if (!s.length) return null; const m = Math.floor(s.length / 2); return +(s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2).toFixed(2); };
const selected = ALL.filter(t => (!only || only.includes(t.id)) && (!kinds || kinds.includes(t.kind)));
await restart();
const overhead = await fixedOverhead();
console.log(`fixed overhead: tools/list ${overhead.toolsListBytes} B (${overhead.tools} tools; beta ${overhead.betaToolsListBytes} B) + instructions ${overhead.instructionsBytes} B vs Bash tool ≈${BASH_TOOL_DEF_BYTES} B → +${overhead.deltaVsBash} B per request`);
const results = [];
for (const task of selected) {
  const started = Date.now();
  let r;
  try { r = await runTask(task); } catch (error) { r = { status: 'error', error: error.message }; }
  results.push({ id: task.id, tool: task.tool, kind: task.kind, source: task.source, question: task.question, truthSource: task.truthSource, seconds: +((Date.now() - started) / 1000).toFixed(1), ...r });
  if (r.status === 'ran') {
    check(`${task.id} ${task.tool}: octocode answer is correct`, r.octocode.ok, r.octocode.ok ? `${r.octocode.calls} calls ${r.octocode.bytes} B ${r.octocode.ms} ms` : r.octocode.fails.join('; '));
    if (!r.competitor.ok) console.log(`INFO [competitors] ${task.id}: competitor answer wrong/incomplete — ${r.competitor.fails.join('; ').slice(0, 200)}`);
  } else if (r.status === 'error') {
    check(`${task.id} ${task.tool}: harness ran the task`, false, r.error);
  } else console.log(`SKIP [competitors] ${task.id} ${task.tool}: ${r.skipReason}`);
}
client.close();
fs.rmSync(CT, { recursive: true, force: true });

const ran = results.filter(r => r.status === 'ran');
const table = ran.map(r => ({
  id: r.id, tool: r.tool, oOk: r.octocode.ok ? 'Y' : 'N', cOk: r.competitor.ok ? 'Y' : 'N',
  oCalls: r.octocode.calls, cCalls: r.competitor.calls, oBytes: r.octocode.bytes, cBytes: r.competitor.bytes, 'bytes×': r.ratios.bytes,
  oMs: r.octocode.ms, cMs: r.competitor.ms, 'ms×': r.ratios.ms,
}));
console.table(table);
const tally = metric => {
  const out = { win: 0, tie: 0, loss: 0 };
  for (const r of ran) {
    const a = r.octocode[metric], b = r.competitor[metric];
    if (metric === 'ok') { if (a && !b) out.win += 1; else if (a === b) out.tie += 1; else out.loss += 1; continue; }
    if (a < b) out.win += 1; else if (a === b) out.tie += 1; else out.loss += 1;
  }
  return out;
};
const perTool = {};
for (const r of ran) (perTool[r.tool] ??= []).push(r);
const toolSummary = Object.fromEntries(Object.entries(perTool).map(([tool, rows]) => [tool, {
  tasks: rows.length, octocodeCorrect: rows.filter(r => r.octocode.ok).length, competitorCorrect: rows.filter(r => r.competitor.ok).length,
  medianCallsRatio: median(rows.map(r => r.ratios.calls)), medianBytesRatio: median(rows.map(r => r.ratios.bytes)), medianMsRatio: median(rows.map(r => r.ratios.ms)),
}]));
console.table(toolSummary);
const overall = {
  tasks: selected.length, ran: ran.length, skipped: results.filter(r => r.status === 'skipped').length, errors: results.filter(r => r.status === 'error').length,
  octocodeCorrect: ran.filter(r => r.octocode.ok).length, competitorCorrect: ran.filter(r => r.competitor.ok).length,
  medianCallsRatio: median(ran.map(r => r.ratios.calls)), medianBytesRatio: median(ran.map(r => r.ratios.bytes)), medianMsRatio: median(ran.map(r => r.ratios.ms)),
  callsLeCompetitor: ran.filter(r => r.octocode.calls <= r.competitor.calls).length,
  bytesLeCompetitor: ran.filter(r => r.octocode.bytes <= r.competitor.bytes).length,
  wins: { correctness: tally('ok'), calls: tally('calls'), bytes: tally('bytes'), ms: tally('ms') },
};
console.log('overall', JSON.stringify(overall));
const { results: checkResults, failed } = summary();
const payload = { at: new Date().toISOString(), node: process.version, overhead, overall, toolSummary, table, tasks: results, checks: checkResults };
const file = writeResults('competitors', payload);
const saveAs = arg('save-as');
if (saveAs) writeResults(saveAs, payload);
console.log(`results: ${file}${saveAs ? ` (+ ${saveAs}.json)` : ''}`);
process.exitCode = failed.length ? 1 : 0;
