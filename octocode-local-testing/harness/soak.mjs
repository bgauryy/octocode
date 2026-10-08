#!/usr/bin/env node
// Soak harness: hunts first-load hangs, crashes, RSS growth and
// latency drift over a long run of realistic calls. Opt-in (run-all: `soak`).
//
//   node harness/soak.mjs --minutes=30            # MCP soak on the built dist (packages/octocode-mcp/dist)
//   node harness/soak.mjs --cli --count=100       # 100 sequential cold CLI starts (packages/octocode/out)
//   node harness/soak.mjs --self-check            # stats unit checks + injected breach/exit detection run
//   flags: --restart-minutes=5 --deadline-s=120 --rss-s=30 --rss-warmup-s=30 --gh-interval-s=10
//          --clasify-interval-s=120 --gap-ms=200 --drift-floor-ms=100 --no-github --light (this-repo local calls only) --out=<dir>
//          test hooks: --inject-slow-ms=N (first call gets an N ms deadline), --inject-kill (SIGKILL the server once)
//
// MCP mode rotates local calls (this repo + octocode-local-testing/repos) with
// GitHub/registry calls at most one per --gh-interval-s and clasify (only when
// registered) at most one per --clasify-interval-s. Every call has a deadline:
// on breach it records the call, runs `sample <pid> 5` into samples/, and
// restarts the server. The server restarts cold every --restart-minutes; an
// exit it did not ask for is recorded and the server restarts. RSS is sampled
// every --rss-s. Output: .octocode/evals/<date>-soak[-cli]/{summary.json,
// summary.md,calls.jsonl,stderr.log,samples/}.
// PASS: no unexpected exit, no deadline breach, RSS growth < +25% per server
// window (first sample after --rss-warmup-s vs last), per-tool p95 drift < 2x
// (first vs last --restart-minutes of the run; skipped when last p95 < floor).
// Exit 0 on PASS, 1 on FAIL.
import { execFile, execFileSync, spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { ROOT, checks, clasifyUnavailable, startServer } from './mcp-client.mjs';

const SELF = fileURLToPath(import.meta.url);
const CLI = path.join(ROOT, 'packages/octocode/out/octocode.js');
const T = 'octocode-local-testing/repos';

// ---------- call mix ----------
// The first five stay in this repo (`--light`); heavy cold LSP indexing comes last
// and `cli: false` keeps it out of the 100-start CLI loop.
const LOCAL = [
  { tool: 'localSearch', args: { matchString: 'startServer', path: 'octocode-local-testing/harness' } },
  { tool: 'localFetch', args: { path: 'octocode-local-testing/harness/mcp-client.mjs', matchString: 'export async function startServer', block: true } },
  { tool: 'structureSearch', args: { operation: 'files', path: 'packages/octocode-config/src' } },
  { tool: 'astSearch', args: { operation: 'symbols', path: 'octocode-local-testing/harness/mcp-client.mjs' } },
  { tool: 'lspSearch', args: { operation: 'documentSymbols', path: 'octocode-local-testing/harness/mcp-client.mjs' } },
  { tool: 'localSearch', args: { matchString: 'budget', path: `${T}/rust/tokio/src`, caseMode: 'insensitive' } },
  { tool: 'localFetch', args: { path: `${T}/rust/tokio/src/runtime/builder.rs`, matchString: 'cannot be set to 0', contextLines: 3 } },
  { tool: 'structureSearch', args: { operation: 'files', path: `${T}/go/scrape`, include: ['*_test.go'] } },
  { tool: 'astSearch', args: { operation: 'match', path: `${T}/rust/tokio/src`, pattern: 'Pin::new_unchecked($A)', language: 'rust' } },
  { tool: 'localSearch', args: { matchString: 'run_on_commit', path: `${T}/python/django/db` } },
  { tool: 'localFetch', args: { path: `${T}/python/django/db/backends/base/base.py`, ranges: ['95-105', '255-265', '340-350', '410-420'] } },
  { tool: 'structureSearch', args: { operation: 'files', path: `${T}/python/django/db`, extensions: ['py'] } },
  { tool: 'astSearch', args: { operation: 'match', path: `${T}/rust/tokio/src/fs`, pattern: '$X.unwrap()', language: 'rust' } },
  { tool: 'localSearch', args: { matchString: 'fn merge_near_windows', path: 'packages/octocode-native/crates' } },
  { tool: 'lspSearch', cli: false, args: { operation: 'references', path: `${T}/rust/tokio/src/runtime/task/harness.rs`, symbolName: 'try_read_output', lineHint: 281 } },
  { tool: 'lspSearch', cli: false, args: { operation: 'callers', path: `${T}/tsx/packages/common/src/url.ts`, symbolName: 'toValidURL', lineHint: 21 } },
];
const STARLETTE = { owner: 'Kludex', repo: 'starlette', ref: '63c5760d8a672cee96e1e523d84bfa1c77d9ee4c' };
const TOKIO_AT = { owner: 'tokio-rs', repo: 'tokio', ref: 'facc6fc47eb2ba13a465a3179f8e34db620561ec' };
const GITHUB = [
  { tool: 'ghSearchRepo', args: { keywords: ['tokio'], language: 'rust', sort: 'stars', pageSize: 5 } },
  { tool: 'ghSearchCode', args: { owner: 'tokio-rs', repo: 'tokio', keywords: ['max_blocking_threads'], pageSize: 5 } },
  { tool: 'ghStructure', args: { ...STARLETTE, path: 'starlette' } },
  { tool: 'ghGetFileContent', args: { ...TOKIO_AT, path: 'tokio/src/runtime/builder.rs', matchString: 'max_blocking_threads: 512', contextLines: 0 } },
  { tool: 'ghSearchHistory', args: { operation: 'commit', owner: 'fastapi', repo: 'fastapi', path: 'fastapi/routing.py', until: '2026-09-01', pageSize: 5 } },
  { tool: 'ghGetHistoryItem', args: { operation: 'pullRequest', owner: 'fastapi', repo: 'fastapi', number: 16403 } },
  { tool: 'artifactSearch', args: { ecosystem: 'npm', packageName: 'express', version: '4.21.2' } },
  { tool: 'ghSearchCode', args: { owner: 'Kludex', repo: 'starlette', keywords: ['wrap_app_handling_exceptions'] } },
  { tool: 'ghStructure', args: { ...STARLETTE, include: ['**/_exception_handler.py'] } },
  { tool: 'ghGetFileContent', args: { ...TOKIO_AT, path: 'tokio/src/runtime/builder.rs', matchString: '^\\s*pub fn \\w+', regex: 'rust', contextLines: 0 } },
  { tool: 'ghSearchHistory', args: { operation: 'pullRequest', owner: 'pydantic', repo: 'pydantic', keywords: ['13786'] } },
  { tool: 'ghGetHistoryItem', args: { operation: 'issue', owner: 'cli', repo: 'cli', number: 14404 } },
  { tool: 'artifactSearch', args: { ecosystem: 'pypi', packageName: 'requests', version: '2.31.0' } },
];
const CLASIFY = [
  { tool: 'clasify', args: { mainGoal: 'Zero worker threads check', reasoning: 'soak', resources: [{ id: 'f', tool: 'localFetch', query: { path: `${T}/rust/tokio/src/runtime/builder.rs` }, prefilter: ['worker_threads'] }], questions: [{ id: 't', type: 'locate', ask: 'Where the runtime Builder rejects a worker thread count of zero' }] } },
];
const INJECTED = { tool: 'localSearch', label: 'inject-slow', args: { matchString: 'budget', path: `${T}/rust/tokio/src`, caseMode: 'insensitive' } };

// ---------- pure stats (self-checked) ----------
export function pct(values, p) {
  if (!values.length) return null;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.max(0, Math.ceil((p / 100) * sorted.length) - 1))];
}

/** Per server window: first RSS sample at least `warmupMs` after start vs the last sample. */
export function rssVerdicts(windows, { warmupMs, limit = 0.25 }) {
  return windows.map(w => {
    const samples = w.rss.filter(s => Number.isFinite(s.rssKb));
    const base = samples.find(s => s.t - w.startedAt >= warmupMs);
    const last = samples.at(-1);
    if (!base || !last || last === base) return { window: w.window, pid: w.pid, ok: null, note: 'too short for a warm baseline' };
    const growth = last.rssKb / base.rssKb - 1;
    return { window: w.window, pid: w.pid, baseKb: base.rssKb, lastKb: last.rssKb, growth: +growth.toFixed(3), ok: growth < limit };
  });
}

/** Per tool: p95 of successful calls in the first vs the last `spanMs` of the run (disjoint spans only). */
export function driftVerdicts(calls, { spanMs, runMs, floorMs, limit = 2, minCalls = 3 }) {
  const tools = [...new Set(calls.map(c => c.tool))];
  return tools.map(tool => {
    const mine = calls.filter(c => c.tool === tool && !c.isError && !c.breach && !c.injected);
    if (runMs < 2 * spanMs) return { tool, ok: null, note: 'run shorter than two spans' };
    const first = mine.filter(c => c.t < spanMs).map(c => c.ms);
    const last = mine.filter(c => c.t >= runMs - spanMs).map(c => c.ms);
    if (first.length < minCalls || last.length < minCalls) return { tool, ok: null, note: `fewer than ${minCalls} calls per span` };
    const firstP95 = pct(first, 95), lastP95 = pct(last, 95);
    const ratio = +(lastP95 / Math.max(1, firstP95)).toFixed(2);
    return { tool, firstP95, lastP95, ratio, ok: lastP95 < floorMs || ratio < limit };
  });
}

export function toolStats(calls) {
  const tools = [...new Set(calls.map(c => c.tool))].sort();
  return tools.map(tool => {
    const mine = calls.filter(c => c.tool === tool);
    const ms = mine.map(c => c.ms);
    return {
      tool, calls: mine.length, errors: mine.filter(c => c.isError).length, breaches: mine.filter(c => c.breach).length,
      p50: pct(ms, 50), p95: pct(ms, 95), max: Math.max(...ms), avgBytes: Math.round(mine.reduce((a, c) => a + (c.bytes ?? 0), 0) / mine.length),
    };
  });
}

// ---------- options ----------
const argv = process.argv.slice(2);
const flag = name => argv.includes(`--${name}`);
const opt = (name, fallback) => { const v = argv.find(a => a.startsWith(`--${name}=`))?.slice(name.length + 3); return v === undefined ? fallback : Number.isNaN(Number(v)) ? v : Number(v); };
const O = {
  minutes: opt('minutes', Number(process.env.OCTOCODE_SOAK_MINUTES ?? 30)),
  restartMs: opt('restart-minutes', 5) * 60_000,
  deadlineMs: opt('deadline-s', 120) * 1000,
  rssMs: opt('rss-s', 30) * 1000,
  warmupMs: opt('rss-warmup-s', 30) * 1000,
  ghMs: Math.max(10, opt('gh-interval-s', 10)) * 1000,
  clasifyMs: opt('clasify-interval-s', 120) * 1000,
  gapMs: opt('gap-ms', 200),
  floorMs: opt('drift-floor-ms', 100),
  count: opt('count', 100),
  injectSlowMs: opt('inject-slow-ms', null),
  injectKill: flag('inject-kill'),
  github: !flag('no-github'),
  cli: flag('cli'),
  light: flag('light'),
};
const DATE = new Date().toISOString().slice(0, 10);
const OUT = path.resolve(ROOT, opt('out', `.octocode/evals/${DATE}-soak${O.cli ? '-cli' : ''}`));
const SAMPLES = path.join(OUT, 'samples');

const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const exists = spec => [spec.args.path].filter(Boolean).every(p => fs.existsSync(path.resolve(ROOT, p)));

function prepareOut() {
  fs.mkdirSync(OUT, { recursive: true });
  for (const f of ['summary.json', 'summary.md', 'calls.jsonl', 'stderr.log']) fs.rmSync(path.join(OUT, f), { force: true });
  fs.rmSync(SAMPLES, { recursive: true, force: true });
  fs.mkdirSync(SAMPLES, { recursive: true });
}
const tee = text => fs.appendFileSync(path.join(OUT, 'stderr.log'), text);
const record = row => fs.appendFileSync(path.join(OUT, 'calls.jsonl'), `${JSON.stringify(row)}\n`);

function rssKb(pid) {
  try { return Number(execFileSync('ps', ['-o', 'rss=', '-p', String(pid)], { encoding: 'utf8' }).trim()) || null; } catch { return null; }
}
/** RSS of the process plus its descendants (LSP servers etc.), informational. */
function treeRssKb(pid) {
  try {
    const rows = execFileSync('ps', ['-A', '-o', 'pid=,ppid=,rss='], { encoding: 'utf8' }).trim().split('\n').map(l => l.trim().split(/\s+/).map(Number));
    const kids = new Map();
    for (const [p, pp] of rows) kids.set(pp, [...(kids.get(pp) ?? []), p]);
    const rss = new Map(rows.map(([p, , r]) => [p, r]));
    let total = 0; const stack = [pid];
    while (stack.length) { const p = stack.pop(); total += rss.get(p) ?? 0; stack.push(...(kids.get(p) ?? [])); }
    return total || null;
  } catch { return null; }
}

/** `sample <pid> 5` (macOS) or a `ps` snapshot elsewhere, into samples/. */
function captureStack(pid, name) {
  const file = path.join(SAMPLES, `${name}.txt`);
  return new Promise(resolve => {
    if (process.platform === 'darwin') {
      execFile('sample', [String(pid), '5', '-file', file], { timeout: 30_000 }, error => {
        if (error && !fs.existsSync(file)) fs.writeFileSync(file, `sample ${pid} failed: ${error.message}\n`);
        resolve(path.relative(OUT, file));
      });
    } else {
      let text = '';
      try { text = execFileSync('ps', ['-o', 'pid,ppid,rss,stat,etime,command', '-p', String(pid)], { encoding: 'utf8' }); } catch (error) { text = error.message; }
      try { text += `\n${fs.readFileSync(`/proc/${pid}/stack`, 'utf8')}`; } catch {}
      fs.writeFileSync(file, text);
      resolve(path.relative(OUT, file));
    }
  });
}

// ---------- MCP soak ----------
async function mcpSoak() {
  prepareOut();
  const runMs = O.minutes * 60_000;
  // startServer's own suite deadline must outlive one window.
  process.env.OCTOCODE_TEST_SUITE_TIMEOUT_MS ??= String(Math.max(runMs, O.restartMs) + 4 * O.deadlineMs + 600_000);
  let stopping = false;
  process.on('SIGINT', () => { stopping = true; });
  process.on('SIGTERM', () => { stopping = true; });

  const startedAt = Date.now();
  const elapsed = () => Date.now() - startedAt;
  const calls = [], events = [], windows = [];
  const local = (O.light ? LOCAL.slice(0, 5) : LOCAL).filter(exists);
  for (const spec of LOCAL.filter(s => !exists(s))) events.push({ type: 'skipped-call', tool: spec.tool, path: spec.args.path });
  let clasifyOn = false, li = 0, gi = 0, ci = 0, lastGh = -Infinity, lastClasify = 0, killed = false, injectPending = O.injectSlowMs != null;
  let srv = null;

  const sampleRss = () => {
    if (!srv || srv.dead) return;
    const row = { t: Date.now(), rssKb: rssKb(srv.pid), treeKb: treeRssKb(srv.pid) };
    srv.window.rss.push(row);
    record({ type: 'rss', window: srv.window.window, pid: srv.pid, elapsedS: +(elapsed() / 1000).toFixed(1), ...row });
  };
  const timer = setInterval(sampleRss, O.rssMs);

  async function boot(reason) {
    for (let attempt = 1; attempt <= 3; attempt++) {
      const t0 = Date.now();
      try {
        const client = await startServer({ cwd: ROOT, timeoutMs: O.deadlineMs * 3 });
        const s = { client, pid: client.server.pid, dead: false, intentional: false };
        s.window = { window: windows.length + 1, pid: s.pid, reason, startedAt: Date.now(), startMs: Date.now() - t0, rss: [] };
        windows.push(s.window);
        tee(`\n=== server #${s.window.window} pid ${s.pid} (${reason}) ${new Date().toISOString()} ===\n${client.stderr()}`);
        client.server.stderr.on('data', chunk => tee(chunk));
        client.server.on('exit', (code, signal) => {
          s.dead = true;
          s.window.endedAt ??= Date.now();
          if (!s.intentional && !stopping) {
            const ev = { type: 'unexpected-exit', window: s.window.window, pid: s.pid, code, signal, elapsedS: +(elapsed() / 1000).toFixed(1), injected: killed && !s.window.endReason };
            s.window.endReason = 'unexpected-exit';
            events.push(ev); record(ev);
            console.log(`EXIT server #${s.window.window} pid ${s.pid} code=${code} signal=${signal}`);
          }
        });
        if (windows.length === 1) {
          clasifyOn = !clasifyUnavailable(client);
          if (!clasifyOn) events.push({ type: 'skipped-tool', tool: 'clasify', reason: clasifyUnavailable(client) });
        }
        console.log(`boot server #${s.window.window} pid ${s.pid} (${reason}) in ${s.window.startMs}ms`);
        srv = s; sampleRss();
        return s;
      } catch (error) {
        const ev = { type: 'start-failed', reason, attempt, error: error.message, elapsedS: +(elapsed() / 1000).toFixed(1) };
        events.push(ev); record(ev);
        console.log(`FAILSTART ${reason} attempt ${attempt}: ${error.message}`);
      }
    }
    return null;
  }

  async function shutdown(s, why) {
    if (!s) return;
    if (!s.dead) sampleRss();
    s.intentional = true;
    s.window.endReason ??= why;
    s.window.endedAt ??= Date.now();
    s.client.close();
    await sleep(300);
  }

  function nextSpec() {
    if (injectPending) { injectPending = false; return { ...INJECTED, injected: true, deadlineMs: O.injectSlowMs }; }
    const now = Date.now();
    if (clasifyOn && now - lastClasify >= O.clasifyMs) { lastClasify = now; return CLASIFY[ci++ % CLASIFY.length]; }
    if (O.github && now - lastGh >= O.ghMs) { lastGh = now; return GITHUB[gi++ % GITHUB.length]; }
    return local[li++ % local.length];
  }

  async function timedCall(s, spec) {
    const deadlineMs = spec.deadlineMs ?? O.deadlineMs;
    const t = elapsed();
    let breachTimer;
    const pending = s.client.call(spec.tool, spec.args, {}, spec.label ?? '');
    const breach = new Promise(resolve => { breachTimer = setTimeout(() => resolve(null), deadlineMs); });
    const entry = await Promise.race([pending, breach]);
    clearTimeout(breachTimer);
    s.client.log.length = 0; // keep harness memory flat
    const row = { type: 'call', i: calls.length + 1, t, elapsedS: +(t / 1000).toFixed(1), window: s.window.window, pid: s.pid, tool: spec.tool, label: spec.label, injected: !!spec.injected };
    if (entry) Object.assign(row, { ms: entry.ms, bytes: entry.bytes, isError: entry.isError, rowErrors: entry.rowErrors, ...(entry.isError && { error: entry.text.slice(0, 300) }) });
    else {
      Object.assign(row, { ms: deadlineMs, breach: true, isError: true, deadlineMs });
      console.log(`BREACH ${spec.tool} after ${deadlineMs}ms (server #${s.window.window} pid ${s.pid}); sampling`);
      row.sample = await captureStack(s.pid, `breach-${row.i}-${spec.tool}`);
      const ev = { type: 'deadline-breach', i: row.i, tool: spec.tool, window: s.window.window, pid: s.pid, deadlineMs, injected: !!spec.injected, sample: row.sample, elapsedS: row.elapsedS };
      events.push(ev); record(ev);
    }
    calls.push(row); record(row);
    return row;
  }

  srv = await boot('initial');
  while (srv && elapsed() < runMs && !stopping) {
    if (srv.dead) { await shutdown(srv, 'unexpected-exit'); srv = await boot('after-exit'); continue; }
    if (Date.now() - srv.window.startedAt >= O.restartMs) { await shutdown(srv, 'scheduled'); srv = await boot('scheduled'); continue; }
    const row = await timedCall(srv, nextSpec());
    if (row.breach) { await shutdown(srv, 'breach'); srv = await boot('after-breach'); continue; }
    if (O.injectKill && !killed && !row.injected && !srv.dead) {
      killed = true;
      console.log(`inject-kill: SIGKILL server pid ${srv.pid}`);
      try { process.kill(srv.pid, 'SIGKILL'); } catch {}
      for (let i = 0; i < 40 && !srv.dead; i++) await sleep(50);
    }
    await sleep(O.gapMs);
  }
  await shutdown(srv, 'end');
  clearInterval(timer);

  const runActualMs = elapsed();
  const rss = rssVerdicts(windows, { warmupMs: O.warmupMs });
  const drift = driftVerdicts(calls, { spanMs: O.restartMs, runMs: runActualMs, floorMs: O.floorMs });
  const exits = events.filter(e => e.type === 'unexpected-exit');
  const breaches = events.filter(e => e.type === 'deadline-breach');
  const startFails = events.filter(e => e.type === 'start-failed');
  const reasons = [];
  if (!calls.length) reasons.push('no calls completed');
  if (exits.length) reasons.push(`${exits.length} unexpected server exit(s): ${exits.map(e => `#${e.window} code=${e.code} signal=${e.signal}${e.injected ? ' (injected)' : ''}`).join(', ')}`);
  if (breaches.length) reasons.push(`${breaches.length} deadline breach(es): ${breaches.map(e => `${e.tool}#${e.i}>${e.deadlineMs}ms${e.injected ? ' (injected)' : ''}`).join(', ')}`);
  if (startFails.length) reasons.push(`${startFails.length} server start failure(s)`);
  for (const r of rss.filter(r => r.ok === false)) reasons.push(`RSS +${Math.round(r.growth * 100)}% in window #${r.window} (${r.baseKb}→${r.lastKb} KB)`);
  for (const d of drift.filter(d => d.ok === false)) reasons.push(`p95 drift ${d.ratio}x for ${d.tool} (${d.firstP95}→${d.lastP95} ms)`);
  const covered = new Set(calls.map(c => c.tool));
  const expected = [...new Set([...local, ...(O.github ? GITHUB : []), ...(clasifyOn ? CLASIFY : [])].map(s => s.tool))];
  const summary = {
    mode: 'mcp', at: new Date().toISOString(), verdict: reasons.length ? 'FAIL' : 'PASS', reasons, options: O, runSeconds: Math.round(runActualMs / 1000),
    calls: calls.length, toolsCovered: [...covered].sort(), toolsNotReached: expected.filter(t => !covered.has(t)),
    tools: toolStats(calls), windows: windows.map(({ rss: samples, ...w }) => ({ ...w, rssSamples: samples.length, firstRssKb: samples[0]?.rssKb, lastRssKb: samples.at(-1)?.rssKb, maxTreeKb: Math.max(0, ...samples.map(s => s.treeKb ?? 0)) })),
    rss, drift, events,
  };
  return finish(summary, [
    ['no unexpected server exit', !exits.length, exits.length ? reasons.find(r => r.includes('exit')) : ''],
    ['no deadline breach', !breaches.length, breaches.length ? reasons.find(r => r.includes('breach')) : ''],
    ['servers start', !startFails.length, startFails.map(e => e.error).join('; ')],
    ['calls completed', calls.length > 0, `${calls.length} calls, ${covered.size} tools`],
    ['RSS growth < +25% per window', rss.every(r => r.ok !== false), rss.filter(r => r.ok !== null).map(r => `#${r.window} ${Math.round(r.growth * 100)}%`).join(' ') || 'n/a (no warm windows)'],
    ['p95 drift < 2x per tool', drift.every(d => d.ok !== false), drift.filter(d => d.ok !== null).map(d => `${d.tool} ${d.ratio}x`).join(' ') || 'n/a (run shorter than two spans)'],
  ]);
}

// ---------- CLI soak ----------
function runCli(spec, deadlineMs, i) {
  return new Promise(resolve => {
    const started = performance.now();
    const child = spawn(process.execPath, [CLI, spec.tool, JSON.stringify({ queries: [spec.args] })], { cwd: ROOT, env: process.env, stdio: ['ignore', 'pipe', 'pipe'], detached: process.platform !== 'win32' });
    let bytes = 0, breach = false, sample;
    child.stdout.on('data', chunk => { bytes += chunk.length; });
    tee(`\n=== cli #${i} ${spec.tool} pid ${child.pid} ===\n`);
    child.stderr.on('data', chunk => tee(chunk));
    const kill = () => { try { process.kill(-child.pid, 'SIGKILL'); } catch { child.kill('SIGKILL'); } };
    const timer = setTimeout(async () => {
      breach = true;
      console.log(`BREACH cli #${i} ${spec.tool} after ${deadlineMs}ms; sampling pid ${child.pid}`);
      sample = await captureStack(child.pid, `cli-breach-${i}-${spec.tool}`);
      kill();
    }, deadlineMs);
    child.on('error', error => { clearTimeout(timer); resolve({ status: null, signal: null, error: error.message, ms: performance.now() - started, bytes, breach, sample }); });
    child.on('exit', (status, signal) => {
      clearTimeout(timer);
      const ms = Math.round(performance.now() - started);
      if (!breach) return resolve({ status, signal, ms, bytes, breach });
      const wait = () => (sample === undefined ? setTimeout(wait, 50) : resolve({ status, signal, ms: deadlineMs, bytes, breach, sample }));
      wait();
    });
  });
}

async function cliSoak() {
  prepareOut();
  const specs = (O.light ? LOCAL.slice(0, 5) : LOCAL).filter(s => s.cli !== false && exists(s));
  const started = Date.now();
  const calls = [];
  for (let i = 1; i <= O.count; i++) {
    const injected = i === 1 && O.injectSlowMs != null;
    const spec = injected ? INJECTED : specs[(i - 1) % specs.length];
    const deadlineMs = injected ? O.injectSlowMs : O.deadlineMs;
    const t = Date.now() - started;
    const r = await runCli(spec, deadlineMs, i);
    const ok = !r.breach && !r.signal && !r.error && (r.status === 0 || r.status === 6);
    const row = { type: 'cli', i, t, elapsedS: +(t / 1000).toFixed(1), tool: spec.tool, injected, ...r, ok, isError: !ok };
    calls.push(row); record(row);
    if (!ok) console.log(`CLI #${i} ${spec.tool}: status=${r.status} signal=${r.signal}${r.breach ? ' BREACH' : ''}${r.error ? ` ${r.error}` : ''}`);
  }
  const runMs = Date.now() - started;
  const bad = calls.filter(c => !c.ok && !c.breach);
  const breaches = calls.filter(c => c.breach);
  // Cold-start drift: first vs last quarter of the run, per tool.
  const drift = driftVerdicts(calls, { spanMs: runMs / 4, runMs, floorMs: O.floorMs, minCalls: 2 });
  const reasons = [];
  if (bad.length) reasons.push(`${bad.length} failing CLI exit(s): ${bad.slice(0, 10).map(c => `#${c.i} ${c.tool} status=${c.status} signal=${c.signal}`).join(', ')}`);
  if (breaches.length) reasons.push(`${breaches.length} deadline breach(es): ${breaches.map(c => `#${c.i} ${c.tool}${c.injected ? ' (injected)' : ''}`).join(', ')}`);
  for (const d of drift.filter(d => d.ok === false)) reasons.push(`p95 drift ${d.ratio}x for ${d.tool} (${d.firstP95}→${d.lastP95} ms)`);
  const summary = {
    mode: 'cli', at: new Date().toISOString(), verdict: reasons.length ? 'FAIL' : 'PASS', reasons, options: O, runSeconds: Math.round(runMs / 1000),
    calls: calls.length, tools: toolStats(calls), drift,
    events: [...bad, ...breaches].map(c => ({ type: c.breach ? 'deadline-breach' : 'bad-exit', i: c.i, tool: c.tool, status: c.status, signal: c.signal, sample: c.sample })),
  };
  return finish(summary, [
    ['CLI exit codes 0/6', !bad.length, `${calls.length - bad.length - breaches.length}/${calls.length} ok`],
    ['no CLI deadline breach', !breaches.length, breaches.map(c => `#${c.i} ${c.tool}`).join(', ')],
    ['CLI p95 drift < 2x per tool', drift.every(d => d.ok !== false), drift.filter(d => d.ok !== null).map(d => `${d.tool} ${d.ratio}x`).join(' ') || 'n/a'],
  ]);
}

// ---------- report ----------
function finish(summary, rows) {
  fs.writeFileSync(path.join(OUT, 'summary.json'), JSON.stringify(summary, null, 2));
  const md = [
    `# Soak ${summary.mode} — ${summary.verdict}`, '',
    `${summary.at} · ${summary.runSeconds}s · ${summary.calls} calls`, '',
    ...(summary.reasons.length ? ['## Reasons', '', ...summary.reasons.map(r => `- ${r}`), ''] : []),
    '## Tools', '', '| tool | calls | errors | breaches | p50 ms | p95 ms | max ms | avg bytes |', '|---|---|---|---|---|---|---|---|',
    ...summary.tools.map(s => `| ${s.tool} | ${s.calls} | ${s.errors} | ${s.breaches} | ${s.p50} | ${s.p95} | ${s.max} | ${s.avgBytes} |`), '',
    ...(summary.windows ? ['## Server windows', '', '| # | pid | start reason | start ms | end | RSS first→last KB | max tree KB | growth |', '|---|---|---|---|---|---|---|---|',
      ...summary.windows.map(w => { const r = summary.rss.find(x => x.window === w.window); return `| ${w.window} | ${w.pid} | ${w.reason} | ${w.startMs} | ${w.endReason ?? ''} | ${w.firstRssKb ?? '?'}→${w.lastRssKb ?? '?'} | ${w.maxTreeKb} | ${r?.ok === null ? 'n/a' : `${Math.round(r.growth * 100)}%`} |`; }), ''] : []),
    '## Drift (p95 first vs last span)', '', ...summary.drift.map(d => `- ${d.tool}: ${d.ok === null ? `n/a (${d.note})` : `${d.firstP95}→${d.lastP95} ms = ${d.ratio}x ${d.ok ? 'ok' : 'FAIL'}`}`), '',
    ...(summary.events.length ? ['## Events', '', ...summary.events.map(e => `- \`${JSON.stringify(e)}\``), ''] : []),
  ];
  fs.writeFileSync(path.join(OUT, 'summary.md'), md.join('\n'));
  const { check } = checks('soak');
  for (const [name, ok, detail] of rows) check(name, ok, detail);
  console.log(`\nsoak ${summary.mode}: ${summary.verdict}${summary.reasons.length ? ` — ${summary.reasons.join('; ')}` : ''}\n${path.relative(ROOT, OUT)}/summary.md`);
  return summary.verdict === 'PASS' ? 0 : 1;
}

// ---------- self-check ----------
function selfCheck() {
  const { check, summary } = checks('soak-self');
  check('pct p95 of 1..100', pct(Array.from({ length: 100 }, (_, i) => i + 1), 95) === 95);
  const w = (rows, startedAt = 0) => ({ window: 1, pid: 1, startedAt, rss: rows.map(([t, rssKb]) => ({ t, rssKb })) });
  check('rss +30% after warmup fails', rssVerdicts([w([[0, 50], [30_000, 100], [290_000, 130]])], { warmupMs: 30_000 })[0].ok === false);
  check('rss +10% after warmup passes (cold-start ramp ignored)', rssVerdicts([w([[0, 50], [30_000, 100], [290_000, 110]])], { warmupMs: 30_000 })[0].ok === true);
  check('rss short window is n/a', rssVerdicts([w([[0, 50], [10_000, 90]])], { warmupMs: 30_000 })[0].ok === null);
  const calls = (ms1, ms2) => [...[1, 2, 3, 4].map(i => ({ tool: 'x', t: i * 1000, ms: ms1 })), ...[1, 2, 3, 4].map(i => ({ tool: 'x', t: 600_000 - i * 1000, ms: ms2 }))];
  check('drift 3x fails', driftVerdicts(calls(200, 600), { spanMs: 300_000, runMs: 600_000, floorMs: 100 })[0].ok === false);
  check('drift 1.5x passes', driftVerdicts(calls(200, 300), { spanMs: 300_000, runMs: 600_000, floorMs: 100 })[0].ok === true);
  check('drift under floor passes', driftVerdicts(calls(10, 50), { spanMs: 300_000, runMs: 600_000, floorMs: 100 })[0].ok === true);
  check('drift on short run is n/a', driftVerdicts(calls(10, 50), { spanMs: 300_000, runMs: 100_000, floorMs: 100 })[0].ok === null);

  // Live: injected tiny deadline + one injected SIGKILL, with fast restarts, must FAIL with both reasons.
  const out = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-soak-self-'));
  const run = spawnSync(process.execPath, [SELF, '--minutes=0.3', '--restart-minutes=0.08', '--rss-s=2', '--gap-ms=500', '--light', '--inject-slow-ms=1', '--inject-kill', '--no-github', `--out=${out}`], { cwd: ROOT, encoding: 'utf8', timeout: 180_000 });
  let s = {};
  try { s = JSON.parse(fs.readFileSync(path.join(out, 'summary.json'), 'utf8')); } catch {}
  const breach = (s.events ?? []).find(e => e.type === 'deadline-breach' && e.injected);
  check('injected slow call exits 1', run.status === 1, `status=${run.status} signal=${run.signal}`);
  check('injected slow call recorded as breach', !!breach && s.reasons?.some(r => r.includes('deadline breach')), JSON.stringify(breach ?? s.reasons ?? run.stdout.slice(-300)));
  const sampleFile = breach?.sample && path.join(out, breach.sample);
  check('breach stack sample written', !!sampleFile && fs.existsSync(sampleFile) && fs.statSync(sampleFile).size > 0, sampleFile);
  check('injected SIGKILL recorded as unexpected exit', (s.events ?? []).some(e => e.type === 'unexpected-exit' && e.signal === 'SIGKILL'), (s.reasons ?? []).join('; '));
  check('server restarted after breach, exit, and schedule', ['after-breach', 'after-exit', 'scheduled'].every(r => (s.windows ?? []).some(w => w.reason === r)), (s.windows ?? []).map(w => w.reason).join(','));
  check('calls.jsonl, stderr.log written', fs.statSync(path.join(out, 'calls.jsonl'), { throwIfNoEntry: false })?.size > 0 && fs.existsSync(path.join(out, 'stderr.log')));
  const cliOut = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-soak-cli-self-'));
  const cliRun = spawnSync(process.execPath, [SELF, '--cli', '--count=3', '--light', '--inject-slow-ms=1', `--out=${cliOut}`], { cwd: ROOT, encoding: 'utf8', timeout: 180_000 });
  let cs = {};
  try { cs = JSON.parse(fs.readFileSync(path.join(cliOut, 'summary.json'), 'utf8')); } catch {}
  check('cli injected slow call is a breach (exit 1)', cliRun.status === 1 && cs.events?.some(e => e.type === 'deadline-breach'), `status=${cliRun.status} ${(cs.reasons ?? []).join('; ')}`);
  check('cli non-injected calls exit 0/6', (cs.events ?? []).every(e => e.type !== 'bad-exit'), (cs.reasons ?? []).join('; '));
  console.log(`self-check outputs: ${out} ${cliOut}`);
  return summary().failed.length ? 1 : 0;
}

if (process.argv[1] && path.resolve(process.argv[1]) === SELF) {
  process.exitCode = flag('self-check') ? selfCheck() : O.cli ? await cliSoak() : await mcpSoak();
  // startServer leaves SIGINT/SIGTERM listeners and timers behind; the verdict is final.
  const code = process.exitCode;
  setTimeout(() => process.exit(code), 200);
}
