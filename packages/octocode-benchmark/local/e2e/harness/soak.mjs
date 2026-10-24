#!/usr/bin/env node
// SOAK, not a correctness test: run-all runs it only when named.
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
// MCP mode rotates local calls (this repo + packages/octocode-benchmark/local/repos) with
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
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { ROOT } from './paths.mjs';
import { checks } from './mcp-client.mjs';
import { isMainModule } from '../../../../../skills/common/scripts/lib/entrypoint.mjs';
import { createOutput, writeReport } from './soak/output.mjs';
import { driftVerdicts, pct, rssVerdicts } from './soak/stats.mjs';
import { mcpSoak } from './soak/mcp-soak.mjs';
import { cliSoak } from './soak/cli-soak.mjs';

const SELF = fileURLToPath(import.meta.url);

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
const output = createOutput(path.resolve(ROOT, opt('out', `.octocode/evals/${DATE}-soak${O.cli ? '-cli' : ''}`)));

// ---------- report ----------
function finish({ summary, rows }) {
  writeReport(output, summary);
  const { check } = checks('soak');
  for (const [name, ok, detail] of rows) check(name, ok, detail);
  console.log(`\nsoak ${summary.mode}: ${summary.verdict}${summary.reasons.length ? ` — ${summary.reasons.join('; ')}` : ''}\n${path.relative(ROOT, output.dir)}/summary.md`);
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
  check('cli non-injected calls exit 0', (cs.events ?? []).every(e => e.type !== 'bad-exit'), (cs.reasons ?? []).join('; '));
  console.log(`self-check outputs: ${out} ${cliOut}`);
  return summary().failed.length ? 1 : 0;
}

// argv[1] keeps a symlinked path while import.meta.url is the real path, so compare real paths.
if (isMainModule(import.meta.url)) {
  const io = { out: output, log: console.log };
  process.exitCode = flag('self-check') ? selfCheck() : finish(O.cli ? await cliSoak(O, io) : await mcpSoak(O, io));
  // startServer leaves SIGINT/SIGTERM listeners and timers behind; the verdict is final.
  const code = process.exitCode;
  setTimeout(() => process.exit(code), 200);
}
