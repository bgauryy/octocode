// Measurement harness for the local-tools head-to-head.
// Every number in REPORT.md comes from records produced here.
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

export const HERE = path.dirname(new URL(import.meta.url).pathname);
export const REPOS = path.resolve(HERE, '../../../repos');
export const BIN = path.join(HERE, 'bin');
const ENV = {
  ...process.env,
  ENABLE_LOCAL: 'true',
  OCTOCODE_BETA: '1',
  PATH: `${BIN}:${path.join(HERE,'tools/node_modules/.bin')}:${process.env.PATH}`,
};

function now() { return Number(process.hrtime.bigint()) / 1e6; }

function runOnce(argv, opts = {}) {
  const t0 = now();
  const r = spawnSync(argv[0], argv.slice(1), {
    env: ENV, encoding: 'utf8', maxBuffer: 1024 * 1024 * 512, cwd: opts.cwd || HERE, timeout: opts.timeout || 300000,
  });
  const ms = now() - t0;
  return { stdout: r.stdout || '', stderr: r.stderr || '', code: r.status, ms };
}

const STARTUP_RE = /fingerprint|contract mismatch|failed to load native|startup/i;

// Run a command `reps` times; record median ms. chars = stdout+stderr of the first run
// (what an agent would read). Retries on native startup/fingerprint errors (rebuild race).
export function measure(argv, { reps = 3, cwd, timeout } = {}) {
  let first;
  for (let attempt = 0; attempt < 6; attempt++) {
    first = runOnce(argv, { cwd, timeout });
    if (argv[0] === 'octocode' && first.code !== 0 && STARTUP_RE.test(first.stderr) && !first.stdout.trim()) {
      spawnSync('sleep', ['60']);
      continue;
    }
    break;
  }
  const times = [first.ms];
  for (let i = 1; i < reps; i++) times.push(runOnce(argv, { cwd, timeout }).ms);
  times.sort((a, b) => a - b);
  return {
    stdout: first.stdout, stderr: first.stderr, code: first.code,
    chars: first.stdout.length + first.stderr.length,
    ms: Math.round(times[Math.floor(times.length / 2)]),
    msAll: times.map(Math.round),
  };
}

export function sh(cmd, opts = {}) {
  const r = measure(['bash', '-c', cmd], opts);
  return { kind: 'shell', cmd, ...r };
}

export function oc(tool, queries, opts = {}) {
  const body = { queries: (Array.isArray(queries) ? queries : [queries]).map(q => ({
    goal: q.goal || 'benchmark task', reasoning: q.reasoning || 'head-to-head validation', ...q })) };
  if (opts.envelope) Object.assign(body, opts.envelope);
  const json = JSON.stringify(body);
  const r = measure(['octocode', tool, json], { cwd: REPOS, ...opts });
  let parsed = null;
  try { parsed = JSON.parse(r.stdout); } catch { /* keep raw */ }
  return { kind: 'octocode', tool, query: body, ...r, parsed };
}

// Replay a raw continuation object verbatim.
export function ocRaw(tool, body, opts = {}) {
  const r = measure(['octocode', tool, JSON.stringify(body)], { cwd: REPOS, ...opts });
  let parsed = null;
  try { parsed = JSON.parse(r.stdout); } catch { }
  return { kind: 'octocode', tool, query: body, ...r, parsed };
}

// Sum a sequence of steps (multi-call flows).
export function flow(steps) {
  return {
    steps,
    calls: steps.length,
    chars: steps.reduce((a, s) => a + s.chars, 0),
    ms: steps.reduce((a, s) => a + s.ms, 0),
  };
}

export function saveRaw(name, data) {
  fs.mkdirSync(path.join(HERE, 'raw'), { recursive: true });
  fs.writeFileSync(path.join(HERE, 'raw', `${name}.json`), JSON.stringify(data, null, 2));
}

// set helpers for precision/recall on "file:line" keys
export function pr(found, truth) {
  const F = new Set(found), T = new Set(truth);
  let tp = 0; for (const x of F) if (T.has(x)) tp++;
  const precision = F.size ? tp / F.size : (T.size ? 0 : 1);
  const recall = T.size ? tp / T.size : 1;
  const missing = [...T].filter(x => !F.has(x));
  const extra = [...F].filter(x => !T.has(x));
  return { found: F.size, truth: T.size, tp, precision: +precision.toFixed(3), recall: +recall.toFixed(3), missing: missing.slice(0, 20), extra: extra.slice(0, 20) };
}

// Parse rg -n output (path:line:text) into keys
export function rgKeys(out, root) {
  const keys = [];
  for (const line of out.split('\n')) {
    const m = line.match(/^(.+?):(\d+):/);
    if (m) keys.push(`${path.relative(root, path.resolve(root, m[1]))}:${m[2]}`);
  }
  return keys;
}

// Extract file:line keys from a localSearch parsed response (all queries)
export function lsKeys(parsed, root) {
  const keys = [];
  for (const r of parsed?.results || []) for (const f of r.data?.files || []) {
    const p = path.isAbsolute(f.path) ? path.relative(root, f.path) : f.path;
    for (const m of f.matches || []) keys.push(`${p}:${m.line}`);
  }
  return keys;
}

// Follow every continuation (response char pages, result pages) verbatim until complete.
// Returns {steps, calls, chars, ms, parts:[parsed...]}
export function ocAll(tool, queries, opts = {}) {
  const steps = [oc(tool, queries, opts)];
  const parts = [steps[0].parsed];
  const maxCalls = opts.maxCalls || 40;
  const queue = [];
  const enqueue = (p) => {
    if (!p) return;
    const rp = p.responsePagination;
    if (rp?.hasMore && rp.next?.query) queue.push(rp.next.query);
    for (const r of p.results || []) {
      const nx = r.data?.next;
      const np = nx?.nextPage || nx?.nextMatchPage || nx?.continueWalk || nx?.nextSymbolPage;
      if (np?.query && (!r.rowPart || r.rowPart.part === 1)) queue.push({ queries: [np.query] });
    }
  };
  enqueue(parts[0]);
  while (queue.length && steps.length < maxCalls) {
    const body = queue.shift();
    const s = ocRaw(tool, body, opts);
    steps.push(s); parts.push(s.parsed); enqueue(s.parsed);
  }
  return { steps: steps.map(s => ({ query: s.query, chars: s.chars, ms: s.ms, code: s.code, stdout: s.stdout.length > 200000 ? s.stdout.slice(0, 200000) + '…[raw truncated in log]' : s.stdout, stderr: s.stderr })),
    calls: steps.length, chars: steps.reduce((a, s) => a + s.chars, 0), ms: steps.reduce((a, s) => a + s.ms, 0), parts, truncatedByCap: queue.length > 0 };
}
