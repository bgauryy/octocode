// Deterministic competitor harness (beat-rg-gh PLAN S5): frozen tasks run as
// an octocode recipe (MCP via the shared client; CLI for CLI-only tools) and
// as the shell recipe an expert agent would run (rg/sed/git/gh/ast-grep/npm).
// Both sides are judged by the same truth check; calls, model-visible bytes
// (structuredContent JSON / stdout+stderr) and wall ms are recorded per side.
// Exit is non-zero when an octocode answer is wrong or a fully walked recipe
// meets a never-trim violation; efficiency losses are reported, not failed.
//
// Fairness (normalized, the default): a local task runs with the MCP workspace
// and the shell cwd at its corpus repo root, so both sides print repo-relative
// paths; queries carry short agent-sized briefs; a task's `unfiltered` shell
// recipe (a first try without answer knowledge: no jq/--json field picks, no
// pre-known line ranges) is reported next to the expert recipe.
// Sensors per octocode response: nextShare (bytes under `next` / response
// bytes), next entries per row, and never-trim (every truncation signal has
// an executable continuation or a terminal-limit disclosure).
//
// Usage: node harness/competitors.mjs [--only=L01,G03] [--kind=local|github|clasify] [--save-as=<results name>]
// Env:   OCTOCODE_COMPETITOR_CLASIFY=1   run the clasify tasks (paid provider calls)
//        OCTOCODE_COMPETITOR_GITHUB=0    skip GitHub/network tasks
//        OCTOCODE_COMPETITOR_NORMALIZE=0 legacy run: octocode-root workspace, question as goal
//        OCTOCODE_COMPETITOR_STRICT_NEXT=1  fail a row with more than NEXT_ENTRY_CAP next entries
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
const NORMALIZED = process.env.OCTOCODE_COMPETITOR_NORMALIZE !== '0';
// Real agent briefs run about 14–32 chars (goal) and 11–25 (reasoning).
const REASONING = NORMALIZED ? 'Need line-level evidence' : 'Answer the task question.';
const NEXT_ENTRY_CAP = 2;
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

// ---------- workspace (F1) ----------
/**
 * The corpus repo a task runs in, relative to ROOT ('' = the octocode root):
 * an explicit `workspace`, else the single `{{T}}/<corpus>` its recipe names.
 */
function taskWorkspace(task) {
  if (!NORMALIZED) return '';
  if (task.workspace != null) return task.workspace;
  const corpora = new Set([...JSON.stringify(task.octocode).matchAll(/\{\{T\}\}\/([\w.-]+)/g)].map(m => m[1]));
  return corpora.size === 1 ? `${T}/${[...corpora][0]}` : '';
}
const escapeRe = s => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
/** Rewrite corpus-prefixed paths to be relative to the task workspace. */
function relativize(text, ws, prefixes) {
  if (!ws) return text;
  const corpus = ws.slice(T.length + 1);
  let out = text;
  for (const prefix of prefixes) {
    const head = escapeRe(`${prefix}/${corpus}`);
    out = out.replace(new RegExp(`${head}/`, 'g'), '').replace(new RegExp(`${head}(?![\\w./-])`, 'g'), '.');
  }
  return out;
}

function sh(step, ws = '') {
  const started = performance.now();
  const cmd = relativize(step.sh, ws, ['$ROOT/$T', '$T']);
  const run = spawnSync('bash', ['-c', `${PREAMBLE}\n${cmd}`], {
    cwd: path.join(ROOT, ws), encoding: 'utf8', maxBuffer: 256 << 20, timeout: 120_000,
    env: { ...process.env, T: ws ? path.join(ROOT, T) : T, CT, ROOT, GH_PAGER: '', NO_COLOR: '1' },
  });
  const ms = Math.round(performance.now() - started);
  const stdout = run.stdout ?? '';
  const stderr = run.stderr ?? '';
  return { cmd, ms, bytes: stdout.length + stderr.length, exit: run.status, stdout, stderr };
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
      case 'files': ev.files.add(l.trim()); ev.fileRows.push(l.trim()); break;
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
// One MCP server per workspace (the octocode root, or a corpus repo root).
const clients = new Map();
async function clientFor(ws) {
  if (!clients.has(ws)) {
    const root = path.join(ROOT, ws);
    clients.set(ws, await startServer({ cwd: root, env: { OCTOCODE_BETA: 'true', ...(ws ? { WORKSPACE_ROOT: root } : {}) } }));
  }
  return clients.get(ws);
}
async function restart(ws) {
  try { clients.get(ws)?.close(); } catch {}
  clients.delete(ws);
  return clientFor(ws);
}

async function mcp(ctx, tool, args) {
  const client = await clientFor(ctx.ws);
  const started = performance.now();
  let response;
  try { response = await client.rpc('tools/call', { name: tool, arguments: args }); } catch (error) { response = { error: { message: error.message } }; }
  const ms = Math.round(performance.now() - started);
  const rawSc = response.result?.structuredContent;
  const text = response.result?.content?.filter(c => c.type === 'text').map(c => c.text).join('') ?? JSON.stringify(response.error ?? '');
  return { surface: 'mcp', tool, args, ms, bytes: rawSc ? JSON.stringify(rawSc).length : text.length, raw: rawSc, sc: expandShared(rawSc), text, isError: !!(response.error || response.result?.isError), transport: response.error?.message };
}

function cli(ctx, tool, args) {
  const started = performance.now();
  const root = path.join(ROOT, ctx.ws);
  const run = spawnSync(process.execPath, [CLI, tool, JSON.stringify(args)], { cwd: root, encoding: 'utf8', maxBuffer: 256 << 20, timeout: 180_000, env: { ...process.env, OCTOCODE_BETA: 'true', ...(ctx.ws ? { WORKSPACE_ROOT: root } : {}) } });
  const ms = Math.round(performance.now() - started);
  const stdout = run.stdout ?? '';
  let sc; try { sc = JSON.parse(stdout); } catch {}
  return { surface: 'cli', tool, args, ms, bytes: stdout.length + (run.stderr ?? '').length, raw: sc, sc, text: stdout + (run.stderr ?? ''), isError: run.status !== 0, transport: run.error?.message };
}

const get = (obj, dotted) => dotted.split('.').reduce((o, k) => (o == null ? undefined : o[k]), obj);
function resolve(value, entries, ws = '') {
  if (typeof value === 'string') {
    return relativize(value, ws, ['{{ROOT}}/{{T}}', '{{T}}']).replace(/\{\{([^}]+)\}\}/g, (_, expr) => {
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
  if (Array.isArray(value)) return value.map(v => resolve(v, entries, ws));
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, resolve(v, entries, ws)]));
  return value;
}

/** Short agent-sized goal (F2): the task's `goal`, else its question. */
const taskGoal = task => (NORMALIZED && task.goal) || task.question;
/** A continuation as call arguments: matrices and envelope continuations are whole calls. */
const asCall = (hint, envelope) => (hint.query.queries || hint.tool === 'clasify' ? hint.query : { ...envelope, queries: [hint.query] });

async function runOctocode(task, ws) {
  const ctx = { ws };
  const goal = taskGoal(task);
  const brief = q => ({ goal, reasoning: REASONING, ...q });
  const entries = [];
  const evidenceSteps = [];
  for (const step of task.octocode) {
    const before = entries.length;
    let exhausted;
    if (step.tool) {
      const queries = step.queries ? step.queries.map(q => brief(resolve(q, entries, ws))) : [brief(resolve(step.args, entries, ws))];
      entries.push(await mcp(ctx, step.tool, { ...step.envelope, queries }));
    } else if (step.raw) {
      entries.push(await mcp(ctx, step.raw, brief(resolve(step.args, entries, ws))));
    } else if (step.cli) {
      entries.push(cli(ctx, step.cli, { queries: [brief(resolve(step.args, entries, ws))] }));
    } else if (step.follow) {
      const hint = entries.slice().reverse().flatMap(e => nextHints(e.sc)).find(h => h.path.endsWith(`.${step.follow}`));
      if (!hint) throw new Error(`no next.${step.follow} to follow`);
      entries.push(await mcp(ctx, hint.tool, asCall(hint, {})));
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
        const e = await mcp(ctx, hint.tool, asCall(hint, step.envelope));
        entries.push(e); calls += 1;
        if (e.isError) break;
        enqueue(e, index);
      }
      exhausted = queue.length === 0;
    }
    evidenceSteps.push({ from: before, to: entries.length, unsearched: !!step.unsearched, walk: !!step.walk, exhausted });
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
    if (typeof node.path === 'string') { here = node.path; if (inArray) { ev.files.add(node.path); ev.fileRows.push(node.path); } }
    if (typeof node.file === 'string') ev.files.add(node.file);
    if (node.from?.path && Array.isArray(node.fromRanges)) { for (const r of node.fromRanges) pair(node.from.path, r.startLine); return; }
    if (node.displayRange && typeof node.path === 'string') pair(node.path, node.displayRange.startLine);
    if (typeof node.line === 'number') pair(here, node.line);
    if (typeof node.content === 'string') numbered(here, node.content);
    if (Array.isArray(node.lines) && node.lines.every(l => typeof l === 'string')) for (const l of node.lines) { const m = /^(\d+)\t/.exec(l); if (m) pair(here, +m[1]); }
    // Compact rows lead with their line: symbols outline "<line>[-<end>] kind name",
    // lean structural matches "<line>[-<end>]\t<value>".
    for (const key of ['declarations', 'matches']) if (Array.isArray(node[key])) for (const row of node[key]) { const m = typeof row === 'string' && /^\s*(\d+)/.exec(row); if (m) pair(here, +m[1]); }
    if (Array.isArray(node.byFile)) for (const f of node.byFile) {
      for (const ref of f.refs ?? []) { const m = /^(\d+)/.exec(ref); if (m) pair(f.path, +m[1]); }
      // Direct callers: "<line>:<col>[,<line>:<col>…] in <kind> <name> …" lists call sites.
      for (const call of f.calls ?? []) for (const site of /^([\d:,]+) in /.exec(call)?.[1].split(',') ?? []) pair(f.path, +site.split(':')[0]);
    }
    if (typeof node.dir === 'string' && Array.isArray(node.files)) {
      // ghStructure dirs are relative to the requested path (the agent's own input).
      const base = entry.tool === 'ghStructure' ? String(entry.args?.queries?.[0]?.path ?? '').replace(/^\.?\/?$/, '') : '';
      const rel = node.dir === '.' || node.dir === '' ? '' : node.dir.replace(/\/$/, '');
      const joined = [base.replace(/\/$/, ''), rel].filter(Boolean).join('/');
      const dir = joined ? `${joined}/` : '';
      for (const name of node.files) if (typeof name === 'string') { ev.files.add(dir + name); ev.fileRows.push(dir + name); }
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
  if (truth.fileCount != null) {
    const n = new Set([...ev.files].map(norm)).size;
    if (n !== truth.fileCount) fails.push(`fileCount ${n} != ${truth.fileCount}`);
  }
  // Every row seen exactly once across all pages: a continuation that skips
  // or repeats rows is a correctness defect, not an efficiency loss.
  if (truth.exactOnce) {
    const keys = truth.exactOnce === 'files' ? ev.fileRows.map(norm) : ev.pairs.map(p => `${norm(p.file)}:${p.line}`);
    const counts = new Map();
    for (const k of keys) counts.set(k, (counts.get(k) ?? 0) + 1);
    const repeated = [...counts].filter(([, n]) => n > 1).map(([k, n]) => `${k}×${n}`);
    if (repeated.length) fails.push(`exactOnce repeated ${repeated.length}: ${repeated.slice(0, 4).join(',')}`);
  }
  for (const re of truth.text ?? []) if (!new RegExp(re).test(ev.text)) fails.push(`text /${re}/ not found`);
  return { ok: fails.length === 0, fails };
}
const emptyEvidence = () => ({ pairs: [], files: new Set(), fileRows: [], pairFiles: new Set(), text: '' });

// ---------- sensors ----------
/** Bytes of every `next` value in a response (key included), envelope pagination too. */
function nextBytes(value) {
  let total = 0;
  const walk = node => {
    if (!node || typeof node !== 'object') return;
    for (const [key, child] of Object.entries(node)) {
      if (key === 'next' && child && typeof child === 'object') { total += JSON.stringify(child).length + 7; continue; }
      walk(child);
    }
  };
  walk(value);
  return total;
}
const isHint = v => !!v && typeof v === 'object' && typeof v.tool === 'string' && v.query && typeof v.query === 'object';
/** `{name, hint}` for each entry of a `next` value (a map of named hints, an array, or one hint). */
function nextEntries(next, name = 'next') {
  if (!next || typeof next !== 'object') return [];
  if (isHint(next)) return [{ name, hint: next }];
  if (Array.isArray(next)) return next.filter(isHint).map(hint => ({ name, hint }));
  return Object.entries(next).filter(([, v]) => isHint(v)).map(([k, hint]) => ({ name: k, hint }));
}
/** Every `next` entry anywhere under a node. */
function allNextEntries(node, out = []) {
  if (!node || typeof node !== 'object') return out;
  for (const [key, child] of Object.entries(node)) {
    if (key === 'next') out.push(...nextEntries(child));
    else allNextEntries(child, out);
  }
  return out;
}
// A paging continuation (it reaches the rest of the same evidence), as
// opposed to a lead to a different read (read, readFixPr, viewRepo, …).
const PAGING_NAME = /page|continue|more|expand|resume|pagination|clasify|^next$/i;
const PAGING_KEYS = ['page', 'offset', 'charOffset', 'matchPage', 'filePage', 'patchPage', 'cursor', 'responseCharOffset', 'after', 'resume'];
const isPaging = ({ name, hint }) => PAGING_NAME.test(name) || PAGING_KEYS.some(k => k in hint.query) || (hint.query.queries ?? []).some(q => PAGING_KEYS.some(k => k in q));
// A query that asked for a window (range, match, block, view) is partial by
// request: its omission markers and isPartial flag are not truncation.
const WINDOW_KEYS = ['matchString', 'ranges', 'startLine', 'endLine', 'block', 'symbol', 'view', 'charLength', 'charOffset', 'offset', 'chunkSize', 'fileFilter', 'matchContext', 'contextLines'];
const OMISSION = /\.\.\. \[lines? \d+(?:-\d+)? omitted\] \.\.\.|\[\.\.\.\s*\d+ (?:more|omitted)[^\]]*\]/;

/** Truncation signals under one row: [{at, signal}], skipping continuation values. */
function trimSignals(node, at, windowed, out = []) {
  if (!node || typeof node !== 'object') return out;
  if (Array.isArray(node)) { node.forEach((child, i) => trimSignals(child, `${at}[${i}]`, windowed, out)); return out; }
  for (const [key, value] of Object.entries(node)) {
    if (key === 'next') continue;
    const here = `${at}.${key}`;
    if ((key === 'hasMore' || key === 'truncated') && value === true) out.push({ at: here, signal: key });
    else if ((key === 'isPartial' || key === 'partial') && value === true && !windowed) out.push({ at: here, signal: key });
    else if (key === 'moreLines' && value) out.push({ at: here, signal: key });
    else if (/^(clipped|omitted)/i.test(key) && value && !(Array.isArray(value) && !value.length)) out.push({ at: here, signal: key });
    else if (typeof value === 'string' && !windowed && OMISSION.test(value)) out.push({ at: here, signal: 'omissionMarker' });
    trimSignals(value, here, windowed, out);
  }
  return out;
}
const hasTerminalLimit = node => JSON.stringify(node ?? null).includes('"terminalLimit":');

/**
 * Per-response sensors: nextShare, next entries per row (max), and never-trim
 * violations (a truncation signal with no executable paging continuation in
 * its row and no terminal-limit disclosure; a split row part needs the
 * envelope continuation).
 */
function responseSensors(entry) {
  const sc = entry.raw;
  const out = { bytes: entry.bytes, nextBytes: 0, nextEntriesMax: 0, nextNames: [], violations: [] };
  if (!sc || typeof sc !== 'object') return out;
  out.nextBytes = nextBytes(sc);
  const queries = Array.isArray(entry.args?.queries) ? entry.args.queries : [entry.args ?? {}];
  const envelopeNext = nextEntries(sc.responsePagination?.next);
  const rows = Array.isArray(sc.results) ? sc.results : [];
  rows.forEach((row, i) => {
    const own = [...nextEntries(row?.next), ...nextEntries(row?.data?.next)];
    out.nextEntriesMax = Math.max(out.nextEntriesMax, own.length);
    const inRow = allNextEntries(row);
    out.nextNames.push(...inRow.map(e => e.name));
    const query = queries[row?.index ?? i] ?? queries[0] ?? {};
    const windowed = WINDOW_KEYS.some(k => k in query);
    const paging = inRow.some(isPaging);
    const disclosed = hasTerminalLimit(row);
    for (const s of trimSignals(row, `results[${i}]`, windowed)) {
      if (!paging && !disclosed) out.violations.push({ tool: entry.tool, ...s, query: JSON.stringify(query).slice(0, 160) });
    }
    if (row?.rowPart && row.rowPart.part < row.rowPart.of && !envelopeNext.length) out.violations.push({ tool: entry.tool, at: `results[${i}].rowPart`, signal: 'rowPart' });
  });
  if (sc.responsePagination?.hasMore && !envelopeNext.length && !hasTerminalLimit(sc.responsePagination)) out.violations.push({ tool: entry.tool, at: 'responsePagination.hasMore', signal: 'hasMore' });
  out.nextNames = [...new Set(out.nextNames)];
  return out;
}

/** Task-level sensors over every octocode response of a recipe. */
function taskSensors(entries, evidenceSteps) {
  const per = entries.map(responseSensors);
  const bytes = per.reduce((a, s) => a + s.bytes, 0);
  const next = per.reduce((a, s) => a + s.nextBytes, 0);
  const walks = evidenceSteps.filter(s => s.walk);
  return {
    nextBytes: next, nextShare: bytes ? +(next / bytes).toFixed(3) : 0,
    nextEntriesMax: Math.max(0, ...per.map(s => s.nextEntriesMax)),
    nextNames: [...new Set(per.flatMap(s => s.nextNames))],
    violations: per.flatMap(s => s.violations),
    // The recipe followed every continuation it walks to the end.
    followedAll: walks.length > 0 && walks.every(s => s.exhausted),
  };
}

// ---------- task runner ----------
let lastCodeSearch = 0;
async function paceCodeSearch() {
  const wait = lastCodeSearch + 7000 - Date.now();
  if (wait > 0) await new Promise(r => setTimeout(r, wait));
  lastCodeSearch = Date.now();
}

async function octocodeSide(task, truth, ws) {
  for (let attempt = 0; attempt < 2; attempt += 1) {
    let run, error;
    try { run = await runOctocode(task, ws); } catch (e) { error = e; }
    const entries = run?.entries ?? [];
    const transient = entries.find(e => (e.transport && TRANSIENT.test(e.transport)) || (e.isError && TRANSIENT.test(e.text.slice(0, 400))));
    if (transient && attempt === 0) { console.log(`RETRY [competitors] ${task.id}: ${String(transient.transport ?? transient.text).slice(0, 120)}; restarting MCP`); await restart(ws); continue; }
    if (entries.some(e => e.isError && RATE_LIMIT.test(e.text))) return { skipped: 'rate limited' };
    const ev = emptyEvidence();
    for (const s of run?.evidenceSteps ?? []) for (const e of entries.slice(s.from, s.to)) octocodeEvidence(e, ev, s);
    const verdict = error ? { ok: false, fails: [error.message] } : evaluate(truth, ev);
    return {
      ...verdict, calls: entries.length, bytes: entries.reduce((a, e) => a + e.bytes, 0), ms: entries.reduce((a, e) => a + e.ms, 0),
      sensors: taskSensors(entries, run?.evidenceSteps ?? []),
      steps: entries.map(e => ({ tool: e.tool, surface: e.surface, ms: e.ms, bytes: e.bytes, nextBytes: nextBytes(e.raw), isError: e.isError })),
      errors: entries.filter(e => e.isError).map(e => e.text.slice(0, 200)),
    };
  }
}

function shellSide(recipe, truth, ws) {
  const ev = emptyEvidence();
  const steps = [];
  for (const step of recipe) {
    const out = sh(step, ws);
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
  const ws = taskWorkspace(task);
  // Local tasks run twice; the second (warm server, warm page cache) is recorded.
  const repeats = task.kind === 'local' ? 2 : 1;
  let octocode, competitor, unfiltered, coldOcto, coldComp;
  for (let i = 0; i < repeats; i += 1) {
    if (task.codeSearch) await paceCodeSearch();
    coldOcto ??= octocode; octocode = await octocodeSide(task, truth, ws);
    if (task.codeSearch) await paceCodeSearch();
    coldComp ??= competitor; competitor = shellSide(task.competitor, truth, ws);
    if (octocode.skipped || competitor.skipped) return { status: 'skipped', skipReason: `rate limited (${octocode.skipped ? 'octocode' : 'competitor'})` };
  }
  // F3: the unfiltered first try, or the expert recipe when it already is one.
  if (task.unfiltered) {
    if (task.codeSearch) await paceCodeSearch();
    unfiltered = shellSide(task.unfiltered, truth, ws);
    if (unfiltered.skipped) return { status: 'skipped', skipReason: 'rate limited (unfiltered)' };
  } else unfiltered = { ...competitor, sameAsExpert: true };
  if (coldOcto) octocode.coldMs = coldOcto.ms;
  if (coldComp) competitor.coldMs = coldComp.ms;
  const ratio = (a, b) => (b > 0 ? +(a / b).toFixed(2) : null);
  return {
    status: 'ran', workspace: ws || '.', truth, octocode, competitor, unfiltered,
    ratios: { calls: ratio(octocode.calls, competitor.calls), bytes: ratio(octocode.bytes, competitor.bytes), ms: ratio(octocode.ms, competitor.ms) },
    unfilteredRatios: { calls: ratio(octocode.calls, unfiltered.calls), bytes: ratio(octocode.bytes, unfiltered.bytes), ms: ratio(octocode.ms, unfiltered.ms) },
  };
}

// ---------- fixed overhead ----------
async function fixedOverhead() {
  const plain = await startServer({ env: { OCTOCODE_BETA: 'false' } });
  const beta = await clientFor('');
  const out = {
    toolsListBytes: JSON.stringify(plain.tools).length, tools: plain.tools.length,
    instructionsBytes: (plain.init?.instructions ?? '').length,
    betaToolsListBytes: JSON.stringify(beta.tools).length, betaTools: beta.tools.length,
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
const overhead = await fixedOverhead();
console.log(`fixed overhead: tools/list ${overhead.toolsListBytes} B (${overhead.tools} tools; beta ${overhead.betaToolsListBytes} B) + instructions ${overhead.instructionsBytes} B vs Bash tool ≈${BASH_TOOL_DEF_BYTES} B → +${overhead.deltaVsBash} B per request`);
console.log(`mode: ${NORMALIZED ? 'normalized (corpus-root workspaces, short briefs)' : 'legacy (octocode-root workspace, question briefs)'}`);
const results = [];
for (const task of selected) {
  const started = Date.now();
  let r;
  try { r = await runTask(task); } catch (error) { r = { status: 'error', error: error.message }; }
  results.push({ id: task.id, tool: task.tool, kind: task.kind, source: task.source, question: task.question, truthSource: task.truthSource, seconds: +((Date.now() - started) / 1000).toFixed(1), ...r });
  if (r.status === 'ran') {
    check(`${task.id} ${task.tool}: octocode answer is correct`, r.octocode.ok, r.octocode.ok ? `${r.octocode.calls} calls ${r.octocode.bytes} B ${r.octocode.ms} ms` : r.octocode.fails.join('; '));
    if (!r.competitor.ok) console.log(`INFO [competitors] ${task.id}: competitor answer wrong/incomplete — ${r.competitor.fails.join('; ').slice(0, 200)}`);
    if (!r.unfiltered.sameAsExpert && !r.unfiltered.ok) console.log(`INFO [competitors] ${task.id}: unfiltered first try wrong/incomplete — ${r.unfiltered.fails.join('; ').slice(0, 200)}`);
    const s = r.octocode.sensors;
    if (s?.violations.length) {
      const detail = s.violations.slice(0, 3).map(v => `${v.tool} ${v.at} (${v.signal})`).join('; ');
      // Only a recipe that walked every continuation to the end proves the
      // remainder is unreachable; otherwise the violation is reported.
      if (s.followedAll) check(`${task.id} ${task.tool}: never-trim (every truncation has a continuation or terminal-limit disclosure)`, false, `${s.violations.length} violations: ${detail}`);
      else console.log(`INFO [competitors] ${task.id}: never-trim violations ${s.violations.length} — ${detail}`);
    }
    if (s && s.nextEntriesMax > NEXT_ENTRY_CAP) {
      const detail = `${s.nextEntriesMax} next entries in one row (cap ${NEXT_ENTRY_CAP}): ${s.nextNames.join(',')}`;
      if (process.env.OCTOCODE_COMPETITOR_STRICT_NEXT === '1') check(`${task.id} ${task.tool}: next entries per row ≤ ${NEXT_ENTRY_CAP}`, false, detail);
      else console.log(`INFO [competitors] ${task.id}: ${detail}`);
    }
  } else if (r.status === 'error') {
    check(`${task.id} ${task.tool}: harness ran the task`, false, r.error);
  } else console.log(`SKIP [competitors] ${task.id} ${task.tool}: ${r.skipReason}`);
}
for (const c of clients.values()) c.close();
fs.rmSync(CT, { recursive: true, force: true });

const ran = results.filter(r => r.status === 'ran');
const table = ran.map(r => ({
  id: r.id, tool: r.tool, oOk: r.octocode.ok ? 'Y' : 'N', cOk: r.competitor.ok ? 'Y' : 'N', uOk: r.unfiltered.sameAsExpert ? '=' : r.unfiltered.ok ? 'Y' : 'N',
  oCalls: r.octocode.calls, cCalls: r.competitor.calls, uCalls: r.unfiltered.calls,
  oBytes: r.octocode.bytes, cBytes: r.competitor.bytes, uBytes: r.unfiltered.bytes, 'bytes×': r.ratios.bytes, 'u bytes×': r.unfilteredRatios.bytes,
  oMs: r.octocode.ms, cMs: r.competitor.ms, 'ms×': r.ratios.ms,
  nextShare: r.octocode.sensors?.nextShare, nextMax: r.octocode.sensors?.nextEntriesMax, trimViol: r.octocode.sensors?.violations.length,
}));
console.table(table);
const tally = (metric, arm = 'competitor') => {
  const out = { win: 0, tie: 0, loss: 0 };
  for (const r of ran) {
    const a = r.octocode[metric], b = r[arm][metric];
    if (metric === 'ok') { if (a && !b) out.win += 1; else if (a === b) out.tie += 1; else out.loss += 1; continue; }
    if (a < b) out.win += 1; else if (a === b) out.tie += 1; else out.loss += 1;
  }
  return out;
};
const perTool = {};
for (const r of ran) (perTool[r.tool] ??= []).push(r);
const toolSummary = Object.fromEntries(Object.entries(perTool).map(([tool, rows]) => [tool, {
  tasks: rows.length, octocodeCorrect: rows.filter(r => r.octocode.ok).length, competitorCorrect: rows.filter(r => r.competitor.ok).length, unfilteredCorrect: rows.filter(r => r.unfiltered.ok).length,
  medianCallsRatio: median(rows.map(r => r.ratios.calls)), medianBytesRatio: median(rows.map(r => r.ratios.bytes)), medianMsRatio: median(rows.map(r => r.ratios.ms)),
  medianBytesRatioUnfiltered: median(rows.map(r => r.unfilteredRatios.bytes)),
  medianNextShare: median(rows.map(r => r.octocode.sensors?.nextShare)), maxNextEntriesPerRow: Math.max(0, ...rows.map(r => r.octocode.sensors?.nextEntriesMax ?? 0)),
  neverTrimViolations: rows.reduce((a, r) => a + (r.octocode.sensors?.violations.length ?? 0), 0),
}]));
console.table(toolSummary);
const overall = {
  mode: NORMALIZED ? 'normalized' : 'legacy',
  tasks: selected.length, ran: ran.length, skipped: results.filter(r => r.status === 'skipped').length, errors: results.filter(r => r.status === 'error').length,
  octocodeCorrect: ran.filter(r => r.octocode.ok).length, competitorCorrect: ran.filter(r => r.competitor.ok).length, unfilteredCorrect: ran.filter(r => r.unfiltered.ok).length,
  medianCallsRatio: median(ran.map(r => r.ratios.calls)), medianBytesRatio: median(ran.map(r => r.ratios.bytes)), medianMsRatio: median(ran.map(r => r.ratios.ms)),
  medianBytesRatioUnfiltered: median(ran.map(r => r.unfilteredRatios.bytes)),
  callsLeCompetitor: ran.filter(r => r.octocode.calls <= r.competitor.calls).length,
  bytesLeCompetitor: ran.filter(r => r.octocode.bytes <= r.competitor.bytes).length,
  bytesLeUnfiltered: ran.filter(r => r.octocode.bytes <= r.unfiltered.bytes).length,
  wins: { correctness: tally('ok'), calls: tally('calls'), bytes: tally('bytes'), ms: tally('ms'), bytesVsUnfiltered: tally('bytes', 'unfiltered') },
  medianNextShare: median(ran.map(r => r.octocode.sensors?.nextShare)),
  rowsOverNextCap: ran.filter(r => (r.octocode.sensors?.nextEntriesMax ?? 0) > NEXT_ENTRY_CAP).map(r => r.id),
  neverTrimViolations: ran.flatMap(r => (r.octocode.sensors?.violations ?? []).map(v => ({ id: r.id, followedAll: r.octocode.sensors.followedAll, ...v }))),
};
console.log('overall', JSON.stringify({ ...overall, neverTrimViolations: overall.neverTrimViolations.length }));
const { results: checkResults, failed } = summary();
const payload = { at: new Date().toISOString(), node: process.version, overhead, overall, toolSummary, table, tasks: results, checks: checkResults };
const file = writeResults('competitors', payload);
const saveAs = arg('save-as');
if (saveAs) writeResults(saveAs, payload);
console.log(`results: ${file}${saveAs ? ` (+ ${saveAs}.json)` : ''}`);
process.exitCode = failed.length ? 1 : 0;
