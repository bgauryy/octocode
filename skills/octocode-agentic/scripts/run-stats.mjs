#!/usr/bin/env node
// Summarizes eval runs per arm and compares two arms with a paired task-cluster bootstrap.

import { readFileSync } from 'node:fs';
import { fail, errorMessage } from './lib/fail.mjs';
import { isMainModule } from './lib/entrypoint.mjs';

const HELP = `Summarize eval runs per arm; with --baseline and --candidate, add paired deltas with bootstrap intervals.

Usage:
  node scripts/run-stats.mjs <runs.jsonl>... [--baseline <arm> --candidate <arm>] [--resamples <n>] [--seed <n>] [--level <0-1>]

Input: JSONL, one task attempt per line:
  {"task":"t1","arm":"A","success":true,"costUsd":0.012,"latencyMs":4100,
   "tokens":{"input":1200,"cacheRead":30000,"cacheWrite":0,"output":800}}
  task and arm are strings; success is true, false or null (unknown: infrastructure, timeout or judge error).
  costUsd covers every request of the attempt, retries and recovery included. costUsd, latencyMs and tokens are
  optional; tokens.input is uncached input only. Repeats of a task share its task id.

Per arm: runs, tasks, successes, failures, unknown, successRate = successes / runs (unknown never counts as success),
costUsd, costPerSuccess = costUsd / successes (null unless every run reports costUsd and one succeeded),
latencyMs {n, p50, p95} (linear interpolation), tokens {input, cacheRead, cacheWrite, output},
cacheHitRate = cacheRead / (input + cacheRead + cacheWrite) (null without token data).

Paired (both --baseline and --candidate): only tasks present in both arms; unpaired lists the rest. Each delta is
candidate - baseline for successRate, costPerSuccess, latencyP50, latencyP95 and cacheHitRate, with a percentile
interval from --resamples (default 10000) cluster resamples of tasks (all repeats of a drawn task stay together),
seeded by --seed (default 1) at --level (default 0.95). undefinedResamples counts resamples where a side had no
value (for example no success); the interval uses the rest.

Output: JSON on stdout. Errors: {ok:false,error:{code,message}}.
Exit codes: 0 summary printed; 2 invalid input (unreadable or malformed lines, unknown arm, no paired tasks, bad options).`;

const isObject = (value) => Boolean(value) && typeof value === 'object' && !Array.isArray(value);
const finite = (value) => typeof value === 'number' && Number.isFinite(value);
const round = (value) => (value === null ? null : Number(value.toFixed(6)));
const BUCKETS = ['input', 'cacheRead', 'cacheWrite', 'output'];

export function parseRuns(text, file = 'runs') {
  const rows = [];
  text.split('\n').forEach((line, index) => {
    if (!line.trim()) return;
    const where = `${file}:${index + 1}`;
    let row;
    try { row = JSON.parse(line); }
    catch { throw new Error(`${where}: not JSON`); }
    if (!isObject(row) || typeof row.task !== 'string' || !row.task || typeof row.arm !== 'string' || !row.arm) throw new Error(`${where}: need task and arm strings`);
    if (![true, false, null].includes(row.success)) throw new Error(`${where}: success must be true, false or null`);
    for (const key of ['costUsd', 'latencyMs']) if (row[key] !== undefined && !(finite(row[key]) && row[key] >= 0)) throw new Error(`${where}: ${key} must be a non-negative number`);
    if (row.tokens !== undefined && !(isObject(row.tokens) && BUCKETS.every((b) => row.tokens[b] === undefined || (finite(row.tokens[b]) && row.tokens[b] >= 0)))) {
      throw new Error(`${where}: tokens holds non-negative ${BUCKETS.join(', ')}`);
    }
    rows.push(row);
  });
  return rows;
}

export function percentile(values, p) {
  if (!values.length) return null;
  const sorted = values.toSorted((a, b) => a - b);
  const position = (sorted.length - 1) * p;
  const low = Math.floor(position);
  const high = Math.ceil(position);
  return sorted[low] + (sorted[high] - sorted[low]) * (position - low);
}

/** Metrics over a list of rows; values are unrounded for resampling. */
function metrics(rows) {
  const successes = rows.filter((r) => r.success === true).length;
  const costRows = rows.filter((r) => r.costUsd !== undefined);
  const costUsd = costRows.reduce((sum, r) => sum + r.costUsd, 0);
  const latencies = rows.filter((r) => r.latencyMs !== undefined).map((r) => r.latencyMs);
  const tokenRows = rows.filter((r) => r.tokens);
  const tokens = Object.fromEntries(BUCKETS.map((b) => [b, tokenRows.reduce((sum, r) => sum + (r.tokens[b] ?? 0), 0)]));
  const cacheBase = tokens.input + tokens.cacheRead + tokens.cacheWrite;
  return {
    runs: rows.length,
    successes,
    successRate: rows.length ? successes / rows.length : null,
    costUsd,
    costRuns: costRows.length,
    costPerSuccess: successes && costRows.length === rows.length ? costUsd / successes : null,
    latencies,
    tokenRuns: tokenRows.length,
    tokens,
    cacheHitRate: tokenRows.length && cacheBase ? tokens.cacheRead / cacheBase : null,
  };
}

function armSummary(rows) {
  const m = metrics(rows);
  return {
    runs: m.runs,
    tasks: new Set(rows.map((r) => r.task)).size,
    successes: m.successes,
    failures: rows.filter((r) => r.success === false).length,
    unknown: rows.filter((r) => r.success === null).length,
    successRate: round(m.successRate),
    costUsd: round(m.costUsd),
    costRuns: m.costRuns,
    costPerSuccess: round(m.costPerSuccess),
    latencyMs: { n: m.latencies.length, p50: round(percentile(m.latencies, 0.5)), p95: round(percentile(m.latencies, 0.95)) },
    tokenRuns: m.tokenRuns,
    tokens: m.tokens,
    cacheHitRate: round(m.cacheHitRate),
  };
}

const DELTAS = {
  successRate: (m) => m.successRate,
  costPerSuccess: (m) => m.costPerSuccess,
  latencyP50: (m) => percentile(m.latencies, 0.5),
  latencyP95: (m) => percentile(m.latencies, 0.95),
  cacheHitRate: (m) => m.cacheHitRate,
};

// mulberry32: a small seeded generator so a rerun reproduces the interval.
function random(seed) {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function deltaOf(name, base, cand) {
  const a = DELTAS[name](base);
  const b = DELTAS[name](cand);
  return a === null || b === null ? null : b - a;
}

export function paired(rows, { baseline, candidate, resamples, seed, level }) {
  const byTask = (arm) => {
    const map = new Map();
    for (const row of rows.filter((r) => r.arm === arm)) map.set(row.task, [...(map.get(row.task) ?? []), row]);
    return map;
  };
  const base = byTask(baseline);
  const cand = byTask(candidate);
  const tasks = [...base.keys()].filter((task) => cand.has(task)).toSorted();
  const unpaired = [...new Set([...base.keys(), ...cand.keys()])].filter((task) => !tasks.includes(task)).toSorted();
  if (!tasks.length) throw new Error(`no task has runs in both ${baseline} and ${candidate}`);
  const pick = (map, chosen) => chosen.flatMap((task) => map.get(task));
  const fullBase = metrics(pick(base, tasks));
  const fullCand = metrics(pick(cand, tasks));
  const draws = Object.fromEntries(Object.keys(DELTAS).map((name) => [name, []]));
  const next = random(seed);
  for (let i = 0; i < resamples; i++) {
    const chosen = Array.from({ length: tasks.length }, () => tasks[Math.floor(next() * tasks.length)]);
    const b = metrics(pick(base, chosen));
    const c = metrics(pick(cand, chosen));
    for (const name of Object.keys(DELTAS)) {
      const delta = deltaOf(name, b, c);
      if (delta !== null) draws[name].push(delta);
    }
  }
  const tail = (1 - level) / 2;
  const deltas = Object.fromEntries(Object.keys(DELTAS).map((name) => [name, {
    baseline: round(DELTAS[name](fullBase)),
    candidate: round(DELTAS[name](fullCand)),
    estimate: round(deltaOf(name, fullBase, fullCand)),
    low: round(percentile(draws[name], tail)),
    high: round(percentile(draws[name], 1 - tail)),
    undefinedResamples: resamples - draws[name].length,
  }]));
  return { baseline, candidate, tasks: tasks.length, unpaired, resamples, seed, level, deltas };
}

export function summarize(rows, options) {
  const arms = [...new Set(rows.map((r) => r.arm))].toSorted();
  const result = { ok: true, arms: Object.fromEntries(arms.map((arm) => [arm, armSummary(rows.filter((r) => r.arm === arm))])) };
  if (options.baseline || options.candidate) {
    for (const arm of [options.baseline, options.candidate]) if (!arms.includes(arm)) throw new Error(`unknown arm ${arm}; arms: ${arms.join(', ')}`);
    result.paired = paired(rows, options);
  }
  return result;
}

function main(argv) {
  if (!argv.length || argv.includes('--help') || argv.includes('-h')) {
    console.log(HELP);
    if (!argv.length) process.exitCode = 2;
    return;
  }
  const options = { resamples: 10000, seed: 1, level: 0.95 };
  const files = [];
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (['--baseline', '--candidate', '--resamples', '--seed', '--level'].includes(arg)) {
      const value = argv[++i];
      if (value === undefined || value.startsWith('--')) fail('invalidArguments', `${arg} requires a value`, 2);
      options[arg.slice(2)] = ['--baseline', '--candidate'].includes(arg) ? value : Number(value);
    } else if (arg.startsWith('--')) fail('invalidArguments', `Unknown option: ${arg} (see --help)`, 2);
    else files.push(arg);
  }
  if (!files.length) fail('invalidArguments', 'give at least one runs JSONL file', 2);
  if (Boolean(options.baseline) !== Boolean(options.candidate) || (options.baseline && options.baseline === options.candidate)) {
    fail('invalidArguments', 'give --baseline and --candidate together, naming two different arms', 2);
  }
  if (!Number.isSafeInteger(options.resamples) || options.resamples < 1) fail('invalidArguments', '--resamples must be a positive integer', 2);
  if (!Number.isSafeInteger(options.seed)) fail('invalidArguments', '--seed must be an integer', 2);
  if (!(options.level > 0 && options.level < 1)) fail('invalidArguments', '--level must be between 0 and 1', 2);
  let result;
  try {
    const rows = files.flatMap((file) => parseRuns(readFileSync(file, 'utf8'), file));
    if (!rows.length) throw new Error('no runs in the input');
    result = summarize(rows, options);
  } catch (error) {
    fail('invalidInput', errorMessage(error), 2);
  }
  console.log(JSON.stringify(result, null, 1));
}

if (isMainModule(import.meta.url)) main(process.argv.slice(2));
