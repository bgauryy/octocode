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
// paths; research tasks carry a short agent-sized `mainGoal` brief and
// single-call lookups none; a task's `unfiltered` shell
// recipe (a first try without answer knowledge: no jq/--json field picks, no
// pre-known line ranges) is reported next to the expert recipe.
// Sensors per octocode response: nextShare / hintsShare (bytes under `next` /
// `hints` over response bytes) and leadShare (lead continuations wherever they
// live), lead entries per menu (cap 2; `next` pages are uncapped), and
// never-trim (every truncation signal has an executable continuation or a
// terminal-limit disclosure). Shape-agnostic: `next` = pages (it also held
// leads in legacy streams), `hints` = leads + text; see harness/sensors.mjs.
// RFC tool-quality-efficiency S1 sensors (harness/sensors.mjs):
//   schema errors   MCP/CLI input-validation errors per task (target 0)
//   verbose fields  default-output fields from harness/verbose-fields.json (report only)
//   verbatim replay a bounded sample of every task's continuations runs unchanged and must validate
//   byte gate       per task vs a pinned anchor and a rolling baseline in results/
//                   (competitors-anchor.json, competitors-rolling.json): bytes or calls
//                   > 1.5× a baseline while the shell ratio worsens fails; local tasks also
//                   fail when unique evidence shrinks or never-trim violations grow. Each
//                   record keeps a body hash and key byte shares to name the growing key.
//
// Usage: node harness/competitors.mjs [--only=L01,G03] [--kind=local|github|clasify] [--save-as=<results name>]
//        [--update-rolling]  accepted run (every check passed): merge this run into the rolling baseline
//        [--update-anchor]   explicit re-anchor: merge this run into the pinned anchor (and rolling);
//                            allowed when only byte-gate checks failed
//        [--self-test]       run the sensor self-tests (no server)
// Env:   OCTOCODE_COMPETITOR_CLASIFY=1   run the clasify tasks (paid provider calls)
//        OCTOCODE_COMPETITOR_GITHUB=0    skip GitHub/network tasks
//        OCTOCODE_COMPETITOR_NORMALIZE=0 legacy run: octocode-root workspace, question as mainGoal on every call
//        OCTOCODE_COMPETITOR_STRICT_NEXT=1  fail a row with more than NEXT_ENTRY_CAP next entries
//        OCTOCODE_COMPETITOR_BASH_TOOL_BYTES  Bash tool definition estimate (default 1200)
//        OCTOCODE_COMPETITOR_REPLAY_MAX=3   continuations replayed verbatim per task (0 = off)
//        RG_BIN                          ripgrep binary (else rg on PATH, else Claude Code's embedded rg)
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { RESULTS, ROOT, checks, cli as runCli, inventoryRows, nextHints, outlineRows, startServer, structureFiles, writeResults } from './mcp-client.mjs';
import {
  ANCHOR_FILE, GATE_FACTOR, ROLLING_FILE, allHintEntries, baselineRecord, bodyHash, bytesUnderKey, callRows, canonical, describeFlag, envelopeContainer,
  gateTask, hintEntries, isHintContainer, isHintKey, isPageName, keyBytes, keyShares, leadBytes, loadVerboseRules, maxLeadEntries, mergeCounts, schemaErrors, selfTest, verboseFields,
} from './sensors.mjs';

if (process.argv.includes('--self-test')) process.exit(selfTest() ? 1 : 0);

const HERE = path.dirname(fileURLToPath(import.meta.url));
const { tasks: ALL } = JSON.parse(fs.readFileSync(path.join(HERE, 'competitor-tasks.json'), 'utf8'));
const T = 'octocode-local-testing/repos';
const CT = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-competitors-'));
const BASH_TOOL_DEF_BYTES = Number(process.env.OCTOCODE_COMPETITOR_BASH_TOOL_BYTES ?? 1200);
const NORMALIZED = process.env.OCTOCODE_COMPETITOR_NORMALIZE !== '0';
// Real agent briefs run about 14–32 chars (mainGoal) and 11–25 (reasoning).
const REASONING = NORMALIZED ? 'Need line-level evidence' : 'Answer the task question.';
const NEXT_ENTRY_CAP = 2;
const REPLAY_MAX = Number(process.env.OCTOCODE_COMPETITOR_REPLAY_MAX ?? 3);
const VERBOSE_RULES = loadVerboseRules();
const sha16 = value => createHash('sha256').update(typeof value === 'string' ? value : JSON.stringify(value)).digest('hex').slice(0, 16);
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
  // Byte gate measure: the compact structuredContent JSON length (text length without it),
  // the payload Claude Code gives the model. OCTOCODE_COMPETITOR_BYTES=text measures the text channel.
  const textBytes = process.env.OCTOCODE_COMPETITOR_BYTES === 'text';
  const e = await client.raw(tool, args, '', { keepRaw: true, bytes: (rawSc, text) => (rawSc && !textBytes ? JSON.stringify(rawSc).length : text.length) });
  return { surface: 'mcp', tool, args, ms: Math.round(e.ms), bytes: e.bytes, raw: e.raw, sc: e.sc, text: e.text, isError: e.isError, transport: e.transport };
}

function cli(ctx, tool, args) {
  const root = path.join(ROOT, ctx.ws);
  const run = runCli(tool, args, { cwd: root, maxBuffer: 256 << 20, timeout: 180_000, env: { OCTOCODE_BETA: 'true', ...(ctx.ws ? { WORKSPACE_ROOT: root } : {}) } });
  let sc; try { sc = JSON.parse(run.stdout); } catch {}
  return { surface: 'cli', tool, args, ms: Math.round(run.ms), bytes: run.stdout.length + run.stderr.length, raw: sc, sc, text: run.stdout + run.stderr, isError: run.status !== 0, transport: run.error?.message };
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

/**
 * The call brief (F2). Briefs are optional: a research task sends its short
 * `mainGoal`, a single-call lookup or page walk sends none. Legacy runs brief
 * every call with the question.
 */
const taskBrief = task => {
  const mainGoal = NORMALIZED ? task.mainGoal : task.question;
  return mainGoal ? { mainGoal, reasoning: REASONING } : {};
};
/**
 * A continuation as call arguments: its query is the complete input. A step's
 * call-level options (e.g. an explicit responseLength) ride along, except on
 * a response page, whose snapshot binds its own window.
 */
const asCall = (hint, options = {}) =>
  hint.query.responseSnapshot ? hint.query : { ...hint.query, ...options };

async function runOctocode(task, ws) {
  const ctx = { ws };
  const taskBriefFields = taskBrief(task);
  const brief = q => ({ ...taskBriefFields, ...q });
  const entries = [];
  const evidenceSteps = [];
  for (const step of task.octocode) {
    const before = entries.length;
    let exhausted;
    if (step.tool) {
      const queries = step.queries ? step.queries.map(q => brief(resolve(q, entries, ws))) : [brief(resolve(step.args, entries, ws))];
      entries.push(await mcp(ctx, step.tool, { ...step.envelope, queries }));
    } else if (step.cli) {
      entries.push(cli(ctx, step.cli, { queries: [brief(resolve(step.args, entries, ws))] }));
    } else if (step.follow) {
      const hint = entries.slice().reverse().flatMap(e => nextHints(e.sc)).find(h => h.path.endsWith(`.${step.follow}`));
      if (!hint) throw new Error(`no next.${step.follow} or hints.${step.follow} to follow`);
      entries.push(await mcp(ctx, hint.tool, asCall(hint, {})));
    } else if (step.walk) {
      // Ordered outer → inner: a response reached through an inner
      // continuation offers only that one or deeper (no page × matchPage fan-out).
      const order = step.walk;
      const seen = new Set();
      const queue = [];
      // Page walkers read `next` only (leads live in `hints`).
      const enqueue = (entry, minIndex) => {
        for (const h of nextHints(entry.sc).filter(x => /\.next(?:\.|$)/.test(x.path))) {
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
    // lspSearch callers: the row `line` is the caller's declaration; `sites` are the call lines.
    if (Array.isArray(node.sites)) { for (const site of node.sites) pair(here, site?.line); }
    else if (typeof node.line === 'number') pair(here, node.line);
    if (typeof node.content === 'string') numbered(here, node.content);
    if (Array.isArray(node.lines) && node.lines.every(l => typeof l === 'string')) for (const l of node.lines) { const m = /^(\d+)\t/.exec(l); if (m) pair(here, +m[1]); }
    // Compact rows: symbols outline rows (grouped and merged, see outlineRows)
    // and lean structural matches "<line>[-<end>]\t<value>".
    if (Array.isArray(node.symbols)) for (const decl of outlineRows(node.symbols)) if (Number.isInteger(decl.line)) pair(here, decl.line);
    if (Array.isArray(node.matches)) for (const row of node.matches) {
      if (typeof row !== 'string') continue;
      // Grouped lspSearch callers: "<line>:<col>[,<line>:<col>…] in <kind> <name> …" lists call sites.
      const sites = /^([\d:,]+) in /.exec(row)?.[1].split(',');
      if (sites) { for (const site of sites) pair(here, +site.split(':')[0]); continue; }
      const m = /^\s*(\d+)/.exec(row); if (m) pair(here, +m[1]);
    }
    if (entry.tool === 'structureSearch' && typeof node.path === 'string' && Array.isArray(node.files)) {
      // structureSearch `files`: bare entries are `path`'s own, group dirs are relative to `path`.
      for (const f of structureFiles(node.files, node.path)) { ev.files.add(f.path); ev.fileRows.push(f.path); }
    } else if (entry.tool === 'structureSearch' && typeof node.dir === 'string' && Array.isArray(node.files)) {
      // Groups were collected with their row above.
    } else if (typeof node.dir === 'string' && Array.isArray(node.files)) {
      // ghStructure `dir` is the repo path; files read "<name> (<bytes>[, <YYYY-MM-DD>])" (GS3).
      const rel = node.dir === '.' || node.dir === '' ? '' : node.dir.replace(/\/$/, '');
      const dir = rel ? `${rel}/` : '';
      for (const entryName of node.files) if (typeof entryName === 'string') {
        const name = entry.tool === 'ghStructure' ? entryName.replace(/ \(\d+(?:, \d{4}-\d{2}-\d{2})?\)$/, '') : entryName;
        ev.files.add(dir + name); ev.fileRows.push(dir + name);
      }
    }
    for (const [key, child] of Object.entries(node)) {
      if (isHintKey(key)) continue;
      if (key === 'unsearchedFiles' && Array.isArray(child)) { if (unsearched) for (const s of child) { const m = /^!\w+ (.+)$/.exec(s); if (m) ev.files.add(m[1]); } continue; }
      if (key === 'files' && entry.tool === 'ghGetHistoryItem' && Array.isArray(child) && child.some(c => typeof c === 'string' || (c && !('path' in c)))) { for (const f of inventoryRows(child)) if (f.path) ev.files.add(f.path); continue; }
      // `enclosing` names the declaration around a hit (X1/X2); its line is not a hit.
      if (key === 'from' || key === 'displayRange' || key === 'enclosing') continue;
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
/** Bytes under every `next` key in a response (key included), envelope pagination too. */
const nextBytes = value => bytesUnderKey(value, 'next');
/** Bytes under every `hints` key (leads and text). */
const hintsBytes = value => bytesUnderKey(value, 'hints');
/** Largest number of entries in any one continuation menu under a node (a row's or an item's). */
function maxNextEntries(node) {
  let max = 0;
  const walk = n => {
    if (!n || typeof n !== 'object') return;
    for (const [key, child] of Object.entries(n)) {
      if (isHintContainer(key, child)) max = Math.max(max, hintEntries(child).length);
      else walk(child);
    }
  };
  walk(node);
  return max;
}
// A paging continuation (it reaches the rest of the same evidence), as
// opposed to a lead to a different read (read, readPullRequest, viewRepo, …).
const PAGING_NAME = /page|continue|more|expand|resume|pagination|clasify|^next$/i;
const PAGING_KEYS = ['page', 'offset', 'matchPage', 'filePage', 'patchPage', 'responseOffset', 'after', 'resume'];
// A core `pages` name (e.g. nextDiagnosticPage) pages its row whatever its spelling.
const isPaging = ({ name, hint }, tool = '') => isPageName(name, tool) || PAGING_NAME.test(name) || PAGING_KEYS.some(k => k in hint.query) || (hint.query.queries ?? []).some(q => PAGING_KEYS.some(k => k in q));
// A query that asked for a window (range, match, block, view) is partial by
// request: its omission markers and isPartial flag are not truncation.
const WINDOW_KEYS = ['matchString', 'ranges', 'block', 'symbol', 'view', 'length', 'offset', 'unit', 'include', 'patchRanges', 'contextLines'];
const OMISSION = /\.\.\. \[(?:lines? \d+(?:-\d+)?|\d+ gaps in lines \d+-\d+) (?:omitted|not requested)\] \.\.\.|\[\.\.\.\s*\d+ (?:more|omitted)[^\]]*\]/;

/** Truncation signals under one row: [{at, signal}], skipping continuation values. */
function trimSignals(node, at, windowed, out = []) {
  if (!node || typeof node !== 'object') return out;
  if (Array.isArray(node)) { node.forEach((child, i) => trimSignals(child, `${at}[${i}]`, windowed, out)); return out; }
  for (const [key, value] of Object.entries(node)) {
    if (isHintKey(key) || key === 'responsePagination') continue;
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
 * Per-response sensors: nextShare, next entries per menu (max), and never-trim
 * violations (a truncation signal with no executable paging continuation in
 * its row and no terminal-limit disclosure; a split row part needs the
 * envelope continuation).
 */
function responseSensors(entry) {
  // nextEntriesMax counts the largest single `next` menu (row- or item-level).
  const sc = entry.raw;
  const out = { bytes: entry.bytes, nextBytes: 0, hintsBytes: 0, leadBytes: 0, nextEntriesMax: 0, leadEntriesMax: 0, nextNames: [], violations: [] };
  if (!sc || typeof sc !== 'object') return out;
  out.nextBytes = nextBytes(sc);
  out.hintsBytes = hintsBytes(sc);
  out.leadBytes = leadBytes(sc, entry.tool);
  const queries = Array.isArray(entry.args?.queries) ? entry.args.queries : [entry.args ?? {}];
  const envelopeNext = hintEntries(envelopeContainer(sc));
  // Tool rows; a clasify matrix reports per-query rows, else the whole response is one row.
  const rows = Array.isArray(sc.results) ? sc.results : Array.isArray(sc.queries) ? sc.queries : [sc];
  rows.forEach((row, i) => {
    out.nextEntriesMax = Math.max(out.nextEntriesMax, maxNextEntries(row));
    out.leadEntriesMax = Math.max(out.leadEntriesMax, maxLeadEntries(row, entry.tool));
    const inRow = allHintEntries(row);
    out.nextNames.push(...inRow.map(e => e.name));
    const query = queries[row?.index ?? i] ?? queries[0] ?? {};
    const windowed = WINDOW_KEYS.some(k => k in query);
    const paging = inRow.some(e => isPaging(e, entry.tool));
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
  const hints = per.reduce((a, s) => a + s.hintsBytes, 0);
  const leads = per.reduce((a, s) => a + s.leadBytes, 0);
  const share = n => (bytes ? +(n / bytes).toFixed(3) : 0);
  const walks = evidenceSteps.filter(s => s.walk);
  return {
    nextBytes: next, nextShare: share(next), hintsBytes: hints, hintsShare: share(hints), leadBytes: leads, leadShare: share(leads),
    nextEntriesMax: Math.max(0, ...per.map(s => s.nextEntriesMax)),
    leadEntriesMax: Math.max(0, ...per.map(s => s.leadEntriesMax)),
    nextNames: [...new Set(per.flatMap(s => s.nextNames))],
    violations: per.flatMap(s => s.violations),
    // The recipe followed every continuation it walks to the end.
    followedAll: walks.length > 0 && walks.every(s => s.exhausted),
    ...s1Sensors(entries),
  };
}

/** S1 sensors over a recipe's responses: schema errors, verbose fields, body hash and key byte shares. */
function s1Sensors(entries) {
  const schema = entries.map(schemaErrors);
  const verbose = {};
  for (const e of entries) mergeCounts(verbose, verboseFields(e, VERBOSE_RULES));
  const kb = keyBytes(entries);
  return {
    schemaErrors: schema.reduce((a, x) => a + x.count, 0),
    schemaErrorDetails: schema.filter(x => x.count).map(x => `${x.codes.join(',')}: ${x.detail}`.slice(0, 200)),
    verbose, verboseBytes: Object.entries(verbose).reduce((a, [k, v]) => a + (k === 'debugRows' ? 0 : v.bytes), 0),
    bodyHash: bodyHash(entries), keyBytes: kb, keyShares: keyShares(kb),
  };
}

/**
 * Verbatim replay (C-3): every continuation offered in a task's responses runs
 * unchanged and must validate. Continuations the recipe already executed
 * count as replayed; up to REPLAY_MAX others (in offer order) are called once.
 */
async function replayHints(entries, ws) {
  const executed = new Set();
  for (const e of entries) {
    executed.add(`${e.tool}:${canonical(e.args)}`);
    for (const q of callRows(e.args)) executed.add(`${e.tool}:${canonical(q)}`);
  }
  const seen = new Set();
  const offered = [];
  for (const e of entries) for (const h of nextHints(e.raw)) {
    const key = `${h.tool}:${canonical(h.query)}`;
    if (seen.has(key)) continue;
    seen.add(key);
    offered.push({ name: h.path.split('.').at(-1), hint: { tool: h.tool, query: h.query }, byRecipe: executed.has(key) });
  }
  // CLI-only tools (ghCloneRepo, astRewrite, astTopology) never list on MCP; they are counted, not replayed.
  const client = await clientFor(ws);
  const onMcp = o => client.tools.some(t => t.name === o.hint.tool);
  const sample = offered.filter(o => !o.byRecipe && onMcp(o)).slice(0, Math.max(0, REPLAY_MAX));
  const replays = [];
  for (const o of sample) {
    if (o.hint.tool === 'ghSearchCode') await paceCodeSearch();
    const e = await mcp({ ws }, o.hint.tool, asCall(o.hint, {}));
    const schema = schemaErrors(e);
    replays.push({ name: o.name, tool: o.hint.tool, surface: e.surface, ok: schema.count === 0, schemaErrors: schema.count, codes: schema.codes, isError: e.isError, rateLimited: e.isError && RATE_LIMIT.test(e.text), bytes: e.bytes, ms: e.ms, detail: schema.count ? schema.detail : undefined });
  }
  return { offered: offered.length, byRecipe: offered.filter(o => o.byRecipe).length, cliOnly: offered.filter(o => !o.byRecipe && !onMcp(o)).length, replayed: replays.length, failed: replays.filter(r => !r.ok).length, replays };
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
    // Coverage facts the byte gate pairs with: unique evidence seen.
    const evidence = { pairs: new Set(ev.pairs.map(p => `${norm(p.file)}:${p.line}`)).size, files: new Set([...ev.files, ...ev.pairFiles, ...ev.pairs.map(p => p.file)].map(norm)).size };
    const out = {
      ...verdict, evidence, calls: entries.length, bytes: entries.reduce((a, e) => a + e.bytes, 0), ms: entries.reduce((a, e) => a + e.ms, 0),
      sensors: taskSensors(entries, run?.evidenceSteps ?? []),
      steps: entries.map(e => ({ tool: e.tool, surface: e.surface, ms: e.ms, bytes: e.bytes, nextBytes: nextBytes(e.raw), hintsBytes: hintsBytes(e.raw), isError: e.isError })),
      errors: entries.filter(e => e.isError).map(e => e.text.slice(0, 200)),
    };
    // Responses for the replay sensor; dropped before results are written.
    Object.defineProperty(out, 'entries', { value: entries, enumerable: false });
    return out;
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
  if (REPLAY_MAX > 0) octocode.replay = await replayHints(octocode.entries, ws);
  const ratio = (a, b) => (b > 0 ? +(a / b).toFixed(2) : null);
  return {
    status: 'ran', workspace: ws || '.', truth, octocode, competitor, unfiltered,
    recipeHash: { octocode: sha16(task.octocode), shell: sha16(task.competitor) },
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
    // R2: the cap applies to lead menus (hints, or today's non-page next entries); pages are uncapped.
    if (s && s.leadEntriesMax > NEXT_ENTRY_CAP) {
      const detail = `${s.leadEntriesMax} lead entries in one menu (cap ${NEXT_ENTRY_CAP}): ${s.nextNames.join(',')}`;
      if (process.env.OCTOCODE_COMPETITOR_STRICT_NEXT === '1') check(`${task.id} ${task.tool}: lead entries per menu ≤ ${NEXT_ENTRY_CAP}`, false, detail);
      else console.log(`INFO [competitors] ${task.id}: ${detail}`);
    }
    if (s?.schemaErrors) check(`${task.id} ${task.tool}: no schema (input-validation) errors`, false, `${s.schemaErrors}: ${s.schemaErrorDetails.join(' | ')}`);
    const rp = r.octocode.replay;
    if (rp?.failed) check(`${task.id} ${task.tool}: continuations replay verbatim (C-3)`, false, rp.replays.filter(x => !x.ok).map(x => `${x.tool} ${x.name}: ${x.detail}`).join(' | '));
    if (s?.verboseBytes) console.log(`INFO [competitors] ${task.id}: verbose default fields ${s.verboseBytes} B — ${Object.entries(s.verbose).filter(([k]) => k !== 'debugRows').map(([k, v]) => `${k}×${v.count} ${v.bytes} B`).join(', ')}`);
  } else if (r.status === 'error') {
    check(`${task.id} ${task.tool}: harness ran the task`, false, r.error);
  } else console.log(`SKIP [competitors] ${task.id} ${task.tool}: ${r.skipReason}`);
}
for (const c of clients.values()) c.close();
fs.rmSync(CT, { recursive: true, force: true });

const ran = results.filter(r => r.status === 'ran');

// ---------- byte-regression gate (pinned anchor + rolling baseline) ----------
const readBaseline = name => { try { return JSON.parse(fs.readFileSync(path.join(RESULTS, name), 'utf8')); } catch { return null; } };
const baselines = { anchor: readBaseline(ANCHOR_FILE), rolling: readBaseline(ROLLING_FILE) };
const gated = [];
for (const r of ran) {
  r.gate = gateTask({ id: r.id, ...baselineRecord(r) }, baselines);
  if (r.gate.flags.length) { gated.push(r.id); check(`byte gate ${r.id} ${r.tool}: within ${GATE_FACTOR}× of anchor and rolling (bytes, calls, evidence)`, false, r.gate.flags.map(describeFlag).join('; ')); }
  else if (r.gate.notes.some(n => /recipe changed/.test(n))) console.log(`INFO [competitors] ${r.id}: ${r.gate.notes.join('; ')}`);
}
if (baselines.anchor || baselines.rolling) check(`byte gate: no task regressed vs ${['anchor', 'rolling'].filter(k => baselines[k]).join(' + ')}`, gated.length === 0, gated.length ? gated.join(',') : `${ran.length} tasks`);
else console.log(`INFO [competitors] byte gate: no baseline yet (seed with --update-anchor)`);
const schemaTotal = ran.reduce((a, r) => a + (r.octocode.sensors?.schemaErrors ?? 0), 0);
check('schema errors: 0 input-validation errors across recipes', schemaTotal === 0, `${schemaTotal}`);
const replayRows = ran.map(r => r.octocode.replay).filter(Boolean);
const replayFailed = replayRows.reduce((a, x) => a + x.failed, 0);
if (REPLAY_MAX > 0) check('verbatim replay: every replayed continuation validates', replayFailed === 0, `${replayRows.reduce((a, x) => a + x.replayed, 0)} replayed + ${replayRows.reduce((a, x) => a + x.byRecipe, 0)} run by recipes of ${replayRows.reduce((a, x) => a + x.offered, 0)} offered; ${replayFailed} failed`);

const table = ran.map(r => ({
  id: r.id, tool: r.tool, oOk: r.octocode.ok ? 'Y' : 'N', cOk: r.competitor.ok ? 'Y' : 'N', uOk: r.unfiltered.sameAsExpert ? '=' : r.unfiltered.ok ? 'Y' : 'N',
  oCalls: r.octocode.calls, cCalls: r.competitor.calls, uCalls: r.unfiltered.calls,
  oBytes: r.octocode.bytes, cBytes: r.competitor.bytes, uBytes: r.unfiltered.bytes, 'bytes×': r.ratios.bytes, 'u bytes×': r.unfilteredRatios.bytes,
  oMs: r.octocode.ms, cMs: r.competitor.ms, 'ms×': r.ratios.ms,
  nextShare: r.octocode.sensors?.nextShare, hintsShare: r.octocode.sensors?.hintsShare, leadMax: r.octocode.sensors?.leadEntriesMax, trimViol: r.octocode.sensors?.violations.length,
  schemaErr: r.octocode.sensors?.schemaErrors, verboseB: r.octocode.sensors?.verboseBytes, replay: r.octocode.replay ? `${r.octocode.replay.replayed - r.octocode.replay.failed}/${r.octocode.replay.replayed}+${r.octocode.replay.byRecipe}` : '', gate: r.gate?.flags.length ? 'FAIL' : r.gate?.notes.length === 2 && r.gate.notes.every(n => /no baseline/.test(n)) ? '—' : 'ok',
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
  medianNextShare: median(rows.map(r => r.octocode.sensors?.nextShare)), medianHintsShare: median(rows.map(r => r.octocode.sensors?.hintsShare)), medianLeadShare: median(rows.map(r => r.octocode.sensors?.leadShare)),
  maxNextEntriesPerRow: Math.max(0, ...rows.map(r => r.octocode.sensors?.nextEntriesMax ?? 0)), maxLeadEntriesPerMenu: Math.max(0, ...rows.map(r => r.octocode.sensors?.leadEntriesMax ?? 0)),
  neverTrimViolations: rows.reduce((a, r) => a + (r.octocode.sensors?.violations.length ?? 0), 0),
  schemaErrors: rows.reduce((a, r) => a + (r.octocode.sensors?.schemaErrors ?? 0), 0),
  verboseBytes: rows.reduce((a, r) => a + (r.octocode.sensors?.verboseBytes ?? 0), 0),
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
  medianHintsShare: median(ran.map(r => r.octocode.sensors?.hintsShare)),
  medianLeadShare: median(ran.map(r => r.octocode.sensors?.leadShare)),
  rowsOverNextCap: ran.filter(r => (r.octocode.sensors?.leadEntriesMax ?? 0) > NEXT_ENTRY_CAP).map(r => r.id),
  neverTrimViolations: ran.flatMap(r => (r.octocode.sensors?.violations ?? []).map(v => ({ id: r.id, followedAll: r.octocode.sensors.followedAll, ...v }))),
  schemaErrors: { total: schemaTotal, byTask: Object.fromEntries(ran.filter(r => r.octocode.sensors?.schemaErrors).map(r => [r.id, r.octocode.sensors.schemaErrors])) },
  verboseFields: (() => {
    const byRule = {};
    for (const r of ran) for (const [k, v] of Object.entries(r.octocode.sensors?.verbose ?? {})) {
      if (k === 'debugRows') continue;
      const x = (byRule[k] ??= { tasks: [], count: 0, bytes: 0 });
      x.tasks.push(r.id); x.count += v.count; x.bytes += v.bytes;
    }
    return { totalBytes: Object.values(byRule).reduce((a, x) => a + x.bytes, 0), byRule };
  })(),
  replay: REPLAY_MAX > 0 ? {
    perTaskMax: REPLAY_MAX,
    offered: replayRows.reduce((a, x) => a + x.offered, 0), byRecipe: replayRows.reduce((a, x) => a + x.byRecipe, 0), cliOnly: replayRows.reduce((a, x) => a + x.cliOnly, 0),
    replayed: replayRows.reduce((a, x) => a + x.replayed, 0), failed: replayFailed, rateLimited: replayRows.reduce((a, x) => a + x.replays.filter(y => y.rateLimited).length, 0),
    runtimeErrors: ran.flatMap(r => (r.octocode.replay?.replays ?? []).filter(y => y.ok && y.isError && !y.rateLimited).map(y => `${r.id} ${y.tool} ${y.name}`)),
  } : null,
  byteGate: { factor: GATE_FACTOR, anchor: baselines.anchor ? { at: baselines.anchor.updatedAt, tasks: Object.keys(baselines.anchor.tasks).length } : null, rolling: baselines.rolling ? { at: baselines.rolling.updatedAt, tasks: Object.keys(baselines.rolling.tasks).length } : null, flagged: gated },
};
console.log('overall', JSON.stringify({ ...overall, neverTrimViolations: overall.neverTrimViolations.length }));
console.table(overall.verboseFields.byRule);
const { results: checkResults, failed } = summary();

// Baselines: rolling only on an accepted run; the anchor only on an explicit flag.
const build = () => {
  const dist = path.join(ROOT, 'packages/octocode-mcp/dist/index.js');
  const addon = fs.readdirSync(path.join(ROOT, 'packages/octocode-native')).find(f => /^octocode-native\..+\.node$/.test(f));
  const stat = addon && fs.statSync(path.join(ROOT, 'packages/octocode-native', addon));
  return { mcpDist: fs.existsSync(dist) ? sha16(fs.readFileSync(dist, 'utf8')) : null, nativeAddon: stat ? { file: addon, bytes: stat.size, mtime: stat.mtime.toISOString() } : null };
};
const writeBaseline = (name, role) => {
  const prev = readBaseline(name);
  const tasks = { ...(prev?.tasks ?? {}) };
  for (const r of ran) tasks[r.id] = baselineRecord(r);
  const env = { clasify: process.env.OCTOCODE_COMPETITOR_CLASIFY === '1', github: process.env.OCTOCODE_COMPETITOR_GITHUB !== '0', normalized: NORMALIZED };
  fs.writeFileSync(path.join(RESULTS, name), JSON.stringify({ version: 1, role, factor: GATE_FACTOR, updatedAt: new Date().toISOString(), seededAt: prev?.seededAt ?? new Date().toISOString(), build: build(), env, tasks }, null, 2));
  console.log(`baseline: wrote ${role} ${name} (${ran.length} tasks updated, ${Object.keys(tasks).length} total)`);
};
const onlyGateFailed = failed.every(f => f.name.startsWith('byte gate'));
if (process.argv.includes('--update-anchor')) {
  if (onlyGateFailed) { writeBaseline(ANCHOR_FILE, 'anchor'); writeBaseline(ROLLING_FILE, 'rolling'); }
  else console.log('baseline: anchor NOT updated — checks other than the byte gate failed');
} else if (process.argv.includes('--update-rolling')) {
  if (!failed.length) writeBaseline(ROLLING_FILE, 'rolling');
  else console.log('baseline: rolling NOT updated — run not accepted');
}
const payload = { at: new Date().toISOString(), node: process.version, overhead, overall, toolSummary, table, tasks: results, checks: checkResults };
const file = writeResults('competitors', payload);
const saveAs = arg('save-as');
if (saveAs) writeResults(saveAs, payload);
console.log(`results: ${file}${saveAs ? ` (+ ${saveAs}.json)` : ''}`);
process.exitCode = failed.length ? 1 : 0;
