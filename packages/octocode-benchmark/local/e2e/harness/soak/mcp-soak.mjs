// MCP soak: rotate the call mix on one server at a time with a deadline per call;
// restart on breach, unexpected exit and schedule; sample RSS; judge exits,
// breaches, RSS growth and p95 drift. `io` = { out (output.mjs), log }.
import { ROOT } from '../paths.mjs';
import { clasifyUnavailable, startServer } from '../mcp-client.mjs';
import { sleep } from '../util.mjs';
import { CLASIFY, GITHUB, INJECTED, LOCAL, exists, localCalls } from './calls.mjs';
import { captureStack, rssKb, treeRssKb } from './probes.mjs';
import { byCodeUnit, driftDetail, driftReasons, driftVerdicts, rssVerdicts, toolStats } from './stats.mjs';

const seconds = ms => +(ms / 1000).toFixed(1);

function createState(O, io) {
  const startedAt = Date.now();
  const st = {
    O, io, elapsed: () => Date.now() - startedAt, calls: [], events: [], windows: [], srv: null, stopping: false, local: localCalls(O.light),
    clasifyOn: false, li: 0, gi: 0, ci: 0, lastGh: -Infinity, lastClasify: 0, killed: false, injectPending: O.injectSlowMs != null,
  };
  for (const spec of LOCAL.filter(s => !exists(s))) st.events.push({ type: 'skipped-call', tool: spec.tool, path: spec.args.path });
  return st;
}

function event(st, ev) {
  st.events.push(ev);
  st.io.out.record(ev);
}

function sampleRss(st) {
  const s = st.srv;
  if (!s || s.dead) return;
  const row = { t: Date.now(), rssKb: rssKb(s.pid), treeKb: treeRssKb(s.pid) };
  s.window.rss.push(row);
  st.io.out.record({ type: 'rss', window: s.window.window, pid: s.pid, elapsedS: seconds(st.elapsed()), ...row });
}

function onExit(st, s, code, signal) {
  s.dead = true;
  s.window.endedAt ??= Date.now();
  if (s.intentional || st.stopping) return;
  const ev = { type: 'unexpected-exit', window: s.window.window, pid: s.pid, code, signal, elapsedS: seconds(st.elapsed()), injected: st.killed && !s.window.endReason };
  s.window.endReason = 'unexpected-exit';
  event(st, ev);
  st.io.log(`EXIT server #${s.window.window} pid ${s.pid} code=${code} signal=${signal}`);
}

/** Track a started server: its window, stderr, exit, and (first server) whether clasify is registered. */
function adopt(st, client, reason, t0) {
  const s = { client, pid: client.server.pid, dead: false, intentional: false };
  s.window = { window: st.windows.length + 1, pid: s.pid, reason, startedAt: Date.now(), startMs: Date.now() - t0, rss: [] };
  st.windows.push(s.window);
  st.io.out.tee(`\n=== server #${s.window.window} pid ${s.pid} (${reason}) ${new Date().toISOString()} ===\n${client.stderr()}`);
  client.server.stderr.on('data', chunk => st.io.out.tee(chunk));
  client.server.on('exit', (code, signal) => onExit(st, s, code, signal));
  if (st.windows.length === 1) {
    const unavailable = clasifyUnavailable(client);
    st.clasifyOn = !unavailable;
    if (unavailable) st.events.push({ type: 'skipped-tool', tool: 'clasify', reason: unavailable });
  }
  st.io.log(`boot server #${s.window.window} pid ${s.pid} (${reason}) in ${s.window.startMs}ms`);
  st.srv = s;
  sampleRss(st);
  return s;
}

async function boot(st, reason) {
  for (let attempt = 1; attempt <= 3; attempt++) {
    const t0 = Date.now();
    try {
      const client = await startServer({ cwd: ROOT, timeoutMs: st.O.deadlineMs * 3 });
      return adopt(st, client, reason, t0);
    } catch (error) {
      event(st, { type: 'start-failed', reason, attempt, error: error.message, elapsedS: seconds(st.elapsed()) });
      st.io.log(`FAILSTART ${reason} attempt ${attempt}: ${error.message}`);
    }
  }
  return null;
}

async function shutdown(st, s, why) {
  if (!s) return;
  if (!s.dead) sampleRss(st);
  s.intentional = true;
  s.window.endReason ??= why;
  s.window.endedAt ??= Date.now();
  s.client.close();
  await sleep(300);
}

async function restart(st, why, reason) {
  await shutdown(st, st.srv, why);
  st.srv = await boot(st, reason);
}

/** The injected slow call first, then clasify and GitHub at most once per interval, else the next local call. */
function nextSpec(st) {
  const { O } = st;
  if (st.injectPending) { st.injectPending = false; return { ...INJECTED, injected: true, deadlineMs: O.injectSlowMs }; }
  const now = Date.now();
  if (st.clasifyOn && now - st.lastClasify >= O.clasifyMs) { st.lastClasify = now; return CLASIFY[st.ci++ % CLASIFY.length]; }
  if (O.github && now - st.lastGh >= O.ghMs) { st.lastGh = now; return GITHUB[st.gi++ % GITHUB.length]; }
  return st.local[st.li++ % st.local.length];
}

async function recordBreach(st, s, row, deadlineMs) {
  Object.assign(row, { ms: deadlineMs, breach: true, isError: true, deadlineMs });
  st.io.log(`BREACH ${row.tool} after ${deadlineMs}ms (server #${s.window.window} pid ${s.pid}); sampling`);
  row.sample = await captureStack(s.pid, `breach-${row.i}-${row.tool}`, st.io.out);
  event(st, { type: 'deadline-breach', i: row.i, tool: row.tool, window: s.window.window, pid: s.pid, deadlineMs, injected: row.injected, sample: row.sample, elapsedS: row.elapsedS });
}

async function timedCall(st, s, spec) {
  const deadlineMs = spec.deadlineMs ?? st.O.deadlineMs;
  const t = st.elapsed();
  let breachTimer;
  const pending = s.client.call(spec.tool, spec.args, {}, spec.label ?? '');
  const breach = new Promise(resolve => { breachTimer = setTimeout(() => resolve(null), deadlineMs); });
  const entry = await Promise.race([pending, breach]);
  clearTimeout(breachTimer);
  s.client.log.length = 0; // keep harness memory flat
  const row = { type: 'call', i: st.calls.length + 1, t, elapsedS: seconds(t), window: s.window.window, pid: s.pid, tool: spec.tool, label: spec.label, injected: !!spec.injected };
  if (entry) Object.assign(row, { ms: entry.ms, bytes: entry.bytes, isError: entry.isError, rowErrors: entry.rowErrors, ...(entry.isError && { error: entry.text.slice(0, 300) }) });
  else await recordBreach(st, s, row, deadlineMs);
  st.calls.push(row);
  st.io.out.record(row);
  return row;
}

async function injectKill(st, srv) {
  st.killed = true;
  st.io.log(`inject-kill: SIGKILL server pid ${srv.pid}`);
  try { process.kill(srv.pid, 'SIGKILL'); } catch { /* already exited */ }
  for (let i = 0; i < 40 && !srv.dead; i++) await sleep(50);
}

/** One loop turn: restart a dead or due server, else one call (a breach restarts), then the gap. */
async function step(st) {
  const srv = st.srv;
  if (srv.dead) { await restart(st, 'unexpected-exit', 'after-exit'); return; }
  if (Date.now() - srv.window.startedAt >= st.O.restartMs) { await restart(st, 'scheduled', 'scheduled'); return; }
  const row = await timedCall(st, srv, nextSpec(st));
  if (row.breach) { await restart(st, 'breach', 'after-breach'); return; }
  if (st.O.injectKill && !st.killed && !row.injected && !srv.dead) await injectKill(st, srv);
  await sleep(st.O.gapMs);
}

function mcpReasons({ calls, exits, breaches, startFails, rss, drift }) {
  const reasons = [];
  if (!calls.length) reasons.push('no calls completed');
  if (exits.length) reasons.push(`${exits.length} unexpected server exit(s): ${exits.map(e => `#${e.window} code=${e.code} signal=${e.signal}${e.injected ? ' (injected)' : ''}`).join(', ')}`);
  if (breaches.length) reasons.push(`${breaches.length} deadline breach(es): ${breaches.map(e => `${e.tool}#${e.i}>${e.deadlineMs}ms${e.injected ? ' (injected)' : ''}`).join(', ')}`);
  if (startFails.length) reasons.push(`${startFails.length} server start failure(s)`);
  for (const r of rss.filter(x => x.ok === false)) reasons.push(`RSS +${Math.round(r.growth * 100)}% in window #${r.window} (${r.baseKb}→${r.lastKb} KB)`);
  reasons.push(...driftReasons(drift));
  return reasons;
}

const windowSummary = ({ rss: samples, ...w }) => ({ ...w, rssSamples: samples.length, firstRssKb: samples[0]?.rssKb, lastRssKb: samples.at(-1)?.rssKb, maxTreeKb: Math.max(0, ...samples.map(s => s.treeKb ?? 0)) });

/** The run's summary and its check rows. */
function verdict(st) {
  const { O, calls, events, windows } = st;
  const runActualMs = st.elapsed();
  const rss = rssVerdicts(windows, { warmupMs: O.warmupMs });
  const drift = driftVerdicts(calls, { spanMs: O.restartMs, runMs: runActualMs, floorMs: O.floorMs });
  const byType = type => events.filter(e => e.type === type);
  const exits = byType('unexpected-exit'), breaches = byType('deadline-breach'), startFails = byType('start-failed');
  const reasons = mcpReasons({ calls, exits, breaches, startFails, rss, drift });
  const covered = new Set(calls.map(c => c.tool));
  const expected = [...new Set([...st.local, ...(O.github ? GITHUB : []), ...(st.clasifyOn ? CLASIFY : [])].map(s => s.tool))];
  const summary = {
    mode: 'mcp', at: new Date().toISOString(), verdict: reasons.length ? 'FAIL' : 'PASS', reasons, options: O, runSeconds: Math.round(runActualMs / 1000),
    calls: calls.length, toolsCovered: [...covered].toSorted(byCodeUnit), toolsNotReached: expected.filter(t => !covered.has(t)),
    tools: toolStats(calls), windows: windows.map(windowSummary), rss, drift, events,
  };
  const rows = [
    ['no unexpected server exit', !exits.length, exits.length ? reasons.find(r => r.includes('exit')) : ''],
    ['no deadline breach', !breaches.length, breaches.length ? reasons.find(r => r.includes('breach')) : ''],
    ['servers start', !startFails.length, startFails.map(e => e.error).join('; ')],
    ['calls completed', calls.length > 0, `${calls.length} calls, ${covered.size} tools`],
    ['RSS growth < +25% per window', rss.every(r => r.ok !== false), rss.filter(r => r.ok !== null).map(r => `#${r.window} ${Math.round(r.growth * 100)}%`).join(' ') || 'n/a (no warm windows)'],
    ['p95 drift < 2x per tool', drift.every(d => d.ok !== false), driftDetail(drift) || 'n/a (run shorter than two spans)'],
  ];
  return { summary, rows };
}

/** Run the MCP soak for `O.minutes`; resolves { summary, rows } for the entry's report. */
export async function mcpSoak(O, io) {
  io.out.prepare();
  const runMs = O.minutes * 60_000;
  // startServer's own suite deadline must outlive one window.
  process.env.OCTOCODE_TEST_SUITE_TIMEOUT_MS ??= String(Math.max(runMs, O.restartMs) + 4 * O.deadlineMs + 600_000);
  const st = createState(O, io);
  process.on('SIGINT', () => { st.stopping = true; });
  process.on('SIGTERM', () => { st.stopping = true; });
  const timer = setInterval(() => sampleRss(st), O.rssMs);
  st.srv = await boot(st, 'initial');
  while (st.srv && st.elapsed() < runMs && !st.stopping) await step(st);
  await shutdown(st, st.srv, 'end');
  clearInterval(timer);
  return verdict(st);
}
