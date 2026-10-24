// CLI soak: `O.count` sequential cold CLI starts with a deadline each (a breach is
// stack-sampled, then killed with its process group); judges exit codes, breaches
// and cold-start p95 drift. `io` = { out (output.mjs), log }.
import { spawn } from 'node:child_process';
import { CLI, ROOT } from '../paths.mjs';
import { INJECTED, localCalls } from './calls.mjs';
import { captureStack } from './probes.mjs';
import { driftDetail, driftReasons, driftVerdicts, toolStats } from './stats.mjs';

/** One detached cold CLI start: resolves {status, signal, ms, bytes, breach, sample?, error?}. */
function coldStart(spec, deadlineMs, i, io) {
  return new Promise(resolve => {
    const started = performance.now();
    const child = spawn(process.execPath, [CLI, spec.tool, JSON.stringify({ queries: [spec.args] })], { cwd: ROOT, env: process.env, stdio: ['ignore', 'pipe', 'pipe'], detached: process.platform !== 'win32' });
    let bytes = 0, breach = false, sample;
    child.stdout.on('data', chunk => { bytes += chunk.length; });
    io.out.tee(`\n=== cli #${i} ${spec.tool} pid ${child.pid} ===\n`);
    child.stderr.on('data', chunk => io.out.tee(chunk));
    const kill = () => { try { process.kill(-Number(child.pid), 'SIGKILL'); } catch { child.kill('SIGKILL'); } };
    const timer = setTimeout(async () => {
      breach = true;
      io.log(`BREACH cli #${i} ${spec.tool} after ${deadlineMs}ms; sampling pid ${child.pid}`);
      sample = await captureStack(child.pid, `cli-breach-${i}-${spec.tool}`, io.out);
      kill();
    }, deadlineMs);
    child.on('error', error => { clearTimeout(timer); resolve({ status: null, signal: null, error: error.message, ms: performance.now() - started, bytes, breach, sample }); });
    child.on('exit', (status, signal) => {
      clearTimeout(timer);
      const ms = Math.round(performance.now() - started);
      if (!breach) { resolve({ status, signal, ms, bytes, breach }); return; }
      const wait = () => (sample === undefined ? setTimeout(wait, 50) : resolve({ status, signal, ms: deadlineMs, bytes, breach, sample }));
      wait();
    });
  });
}

function cliReasons(bad, breaches, drift) {
  const reasons = [];
  if (bad.length) reasons.push(`${bad.length} failing CLI exit(s): ${bad.slice(0, 10).map(c => `#${c.i} ${c.tool} status=${c.status} signal=${c.signal}`).join(', ')}`);
  if (breaches.length) reasons.push(`${breaches.length} deadline breach(es): ${breaches.map(c => `#${c.i} ${c.tool}${c.injected ? ' (injected)' : ''}`).join(', ')}`);
  reasons.push(...driftReasons(drift));
  return reasons;
}

/** Run the CLI soak; resolves { summary, rows } for the entry's report. */
export async function cliSoak(O, io) {
  io.out.prepare();
  const specs = localCalls(O.light).filter(s => s.cli !== false);
  const started = Date.now();
  const calls = [];
  for (let i = 1; i <= O.count; i++) {
    const injected = i === 1 && O.injectSlowMs != null;
    const spec = injected ? INJECTED : specs[(i - 1) % specs.length];
    const t = Date.now() - started;
    const r = await coldStart(spec, injected ? O.injectSlowMs : O.deadlineMs, i, io);
    const ok = !r.breach && !r.signal && !r.error && r.status === 0;
    const row = { type: 'cli', i, t, elapsedS: +(t / 1000).toFixed(1), tool: spec.tool, injected, ...r, ok, isError: !ok };
    calls.push(row);
    io.out.record(row);
    if (!ok) io.log(`CLI #${i} ${spec.tool}: status=${r.status} signal=${r.signal}${r.breach ? ' BREACH' : ''}${r.error ? ` ${r.error}` : ''}`);
  }
  const runMs = Date.now() - started;
  const bad = calls.filter(c => !c.ok && !c.breach);
  const breaches = calls.filter(c => c.breach);
  // Cold-start drift: first vs last quarter of the run, per tool.
  const drift = driftVerdicts(calls, { spanMs: runMs / 4, runMs, floorMs: O.floorMs, minCalls: 2 });
  const reasons = cliReasons(bad, breaches, drift);
  const summary = {
    mode: 'cli', at: new Date().toISOString(), verdict: reasons.length ? 'FAIL' : 'PASS', reasons, options: O, runSeconds: Math.round(runMs / 1000),
    calls: calls.length, tools: toolStats(calls), drift,
    events: [...bad, ...breaches].map(c => ({ type: c.breach ? 'deadline-breach' : 'bad-exit', i: c.i, tool: c.tool, status: c.status, signal: c.signal, sample: c.sample })),
  };
  const rows = [
    ['CLI exit code 0', !bad.length, `${calls.length - bad.length - breaches.length}/${calls.length} ok`],
    ['no CLI deadline breach', !breaches.length, breaches.map(c => `#${c.i} ${c.tool}`).join(', ')],
    ['CLI p95 drift < 2x per tool', drift.every(d => d.ok !== false), driftDetail(drift) || 'n/a'],
  ];
  return { summary, rows };
}
