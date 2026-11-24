#!/usr/bin/env node
// Decide whether two builds differ by more than their run-to-run spread, or how a cost grows with size or cycles.
import { spawn } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { isMainModule } from './lib/entrypoint.mjs';
import { errorMessage, fail } from './lib/fail.mjs';

const USAGE = `Usage:
  node compare-runs.mjs --old <command> --new <command> --rounds <n> [--warmup <n>] [--parse <regex>] [--cwd <dir>]
  node compare-runs.mjs --file <rounds.json|.jsonl|.csv|-> [--base <variant>]
  node compare-runs.mjs --file <points.json|.jsonl|.csv|-> --growth size|cycle

Compare (driver): runs each command with /bin/sh in alternating rounds (old, new, old, new ...) after --warmup unrecorded
  pairs, and records per round wallMs (elapsed), cpuMs (user + system CPU of the command and the children it waited
  for, from the shell's times builtin) and, with --parse, parsed: the number in the regex's first capture group of the
  command's stdout (bytes or calls per operation the command prints). Command output is not echoed.
Compare (file): rows are objects with "variant" and numeric metric fields (every number except round, exitCode, size
  and cycle); rows with "warmup": true are skipped. Exactly two variants. The base is --base, else "old" when
  present, else the first variant seen. A driver result's "rounds" array is valid input.
  For each metric and variant: values in round order, median, min, max and spread (max - min). difference is the other
  median minus the base median, relativePercent that difference over the base median. exceedsSpread is true when
  |difference| > the larger of the two spreads; verdict is then "lower" or "higher" (the other variant against the
  base), else "within-spread"; "insufficient-rounds" when a variant has fewer than 2 rounds.
Growth --growth size: rows with "size" > 0 and positive metrics (per "variant" when present). Per size: the median.
  exponent is the least-squares slope of log(median) against log(size), with r2; nearestPower rounds it (0 constant,
  1 linear, 2 quadratic) once there are 3 or more sizes, else null.
Growth --growth cycle: rows with "cycle" and metric values sampled at the same lifecycle point (heap after GC).
  perCycle is the least-squares slope over all points and latePerCycle over the later half of the cycles. verdict:
  "growing" when the later half rises more than its fit's residual range (a leak keeps rising), "plateau" when only the
  whole series does (a cache fills, then stops), "falling", "flat", or "insufficient-cycles" below 6 cycles.
Output: one JSON object on stdout ({"ok":true,"mode":...}). Effects: the driver runs the given commands in --cwd
  (default: the current directory); file modes only read.
Exit 0 with the analysis (whatever the verdict); 1 when a driven command fails, cannot start or --parse finds no
number; 2 for invalid options or input. Failures print {"ok":false,"error":{code,message}}.`;

const KEYS = new Set(['round', 'exitCode', 'size', 'cycle']);

function parseArgs(argv) {
  const options = { warmup: 0, cwd: process.cwd() };
  const count = (flag, value, min) => {
    const number = Number(value);
    if (!Number.isInteger(number) || number < min) fail('invalid-option', `${flag}: an integer of at least ${min}`, 2);
    return number;
  };
  for (let i = 0; i < argv.length; i++) {
    const flag = argv[i];
    if (flag === '--help') {
      console.log(USAGE);
      process.exit(0);
    }
    const value = argv[++i];
    if (value === undefined) fail('invalid-option', `${flag} needs a value; see --help`, 2);
    if (flag === '--old' || flag === '--new' || flag === '--file' || flag === '--base' || flag === '--cwd') options[flag.slice(2)] = value;
    else if (flag === '--rounds') options.rounds = count(flag, value, 1);
    else if (flag === '--warmup') options.warmup = count(flag, value, 0);
    else if (flag === '--growth' && (value === 'size' || value === 'cycle')) options.growth = value;
    else if (flag === '--parse') {
      try {
        options.parse = new RegExp(value);
      } catch (error) {
        fail('invalid-option', `--parse: ${errorMessage(error)}`, 2);
      }
    } else fail('invalid-option', `unknown option ${flag} ${value}; see --help`, 2);
  }
  const driver = options.old !== undefined || options.new !== undefined;
  if (driver && (options.old === undefined || options.new === undefined || !options.rounds)) fail('invalid-option', '--old, --new and --rounds go together', 2);
  if (driver === (options.file !== undefined)) fail('invalid-option', 'give either --old/--new/--rounds or --file; see --help', 2);
  if (options.growth && driver) fail('invalid-option', '--growth reads --file', 2);
  return options;
}

const median = (values) => {
  const sorted = values.toSorted((a, b) => a - b);
  const middle = sorted.length >> 1;
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
};
const round = (value, digits = 3) => (Number.isFinite(value) ? Number(value.toFixed(digits)) : null);

/** Least-squares line through points [x, y]: { slope, intercept, r2, residuals }. */
export function fit(points) {
  const n = points.length;
  const mx = points.reduce((s, [x]) => s + x, 0) / n;
  const my = points.reduce((s, [, y]) => s + y, 0) / n;
  const sxx = points.reduce((s, [x]) => s + (x - mx) ** 2, 0);
  const sxy = points.reduce((s, [x, y]) => s + (x - mx) * (y - my), 0);
  const slope = sxx > 0 ? sxy / sxx : 0;
  const intercept = my - slope * mx;
  const residuals = points.map(([x, y]) => y - (intercept + slope * x));
  const ssTotal = points.reduce((s, [, y]) => s + (y - my) ** 2, 0);
  const ssResidual = residuals.reduce((s, r) => s + r * r, 0);
  return { slope, intercept, r2: ssTotal > 0 ? 1 - ssResidual / ssTotal : 1, residuals };
}

const range = (values) => Math.max(...values) - Math.min(...values);

/** Rows from JSON (an array, or an object with "rounds"), JSON lines or CSV with a header. */
export function parseRows(text) {
  const trimmed = text.trim();
  if (!trimmed) throw new Error('no rows');
  if (trimmed.startsWith('[') || trimmed.startsWith('{')) {
    let value;
    try {
      value = JSON.parse(trimmed);
    } catch (error) {
      if (!trimmed.startsWith('{')) throw error;
      return trimmed.split('\n').filter((line) => line.trim()).map((line) => JSON.parse(line));
    }
    const rows = Array.isArray(value) ? value : value?.rounds;
    if (!Array.isArray(rows)) throw new Error('a JSON object needs a "rounds" array');
    return rows;
  }
  const [header, ...lines] = trimmed.split(/\r?\n/).filter((line) => line.trim()).map(csvFields);
  return lines.map((fields) => {
    if (fields.length !== header.length) throw new Error(`CSV row has ${fields.length} fields, header has ${header.length}`);
    return Object.fromEntries(header.map((name, i) => [name, csvValue(fields[i])]));
  });
}

function csvFields(line) {
  const fields = [];
  let field = '';
  let quoted = false;
  for (let i = 0; i < line.length; i++) {
    const char = line[i];
    if (quoted && char === '"' && line[i + 1] === '"') field += line[++i];
    else if (char === '"') quoted = !quoted;
    else if (char === ',' && !quoted) {
      fields.push(field);
      field = '';
    } else field += char;
  }
  return [...fields, field].map((value) => value.trim());
}

function csvValue(text) {
  if (/^-?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$/.test(text)) return Number(text);
  if (text === 'true' || text === 'false') return text === 'true';
  return text;
}

const metricNames = (rows) => [...new Set(rows.flatMap((row) => Object.keys(row).filter((key) => !KEYS.has(key) && typeof row[key] === 'number')))];

function describe(values) {
  if (!values.length) return { n: 0, values };
  return { n: values.length, values, median: round(median(values)), min: Math.min(...values), max: Math.max(...values), spread: round(range(values)) };
}

/** Compares two variants of every metric; `base` names the baseline variant. */
export function compare(allRows, base) {
  const rows = allRows.filter((row) => row.warmup !== true);
  const variants = [...new Set(rows.map((row) => row.variant))];
  if (variants.some((variant) => typeof variant !== 'string' || !variant)) throw new Error('every row needs a "variant" name');
  if (variants.length !== 2) throw new Error(`need exactly two variants, found ${variants.length}: ${variants.join(', ')}`);
  const baseline = base ?? (variants.includes('old') ? 'old' : variants[0]);
  if (!variants.includes(baseline)) throw new Error(`--base ${baseline} is not one of: ${variants.join(', ')}`);
  const other = variants.find((variant) => variant !== baseline);
  const names = metricNames(rows);
  if (!names.length) throw new Error('no numeric metric fields');
  const metrics = {};
  for (const name of names) {
    const of = (variant) => describe(rows.filter((row) => row.variant === variant && Number.isFinite(row[name])).map((row) => row[name]));
    const a = of(baseline);
    const b = of(other);
    if (a.n < 2 || b.n < 2) {
      metrics[name] = { [baseline]: a, [other]: b, verdict: 'insufficient-rounds' };
      continue;
    }
    const difference = b.median - a.median;
    const exceedsSpread = Math.abs(difference) > Math.max(a.spread, b.spread);
    metrics[name] = {
      [baseline]: a,
      [other]: b,
      difference: round(difference),
      relativePercent: a.median !== 0 ? round((100 * difference) / a.median, 2) : null,
      exceedsSpread,
      verdict: exceedsSpread ? (difference < 0 ? 'lower' : 'higher') : 'within-spread',
    };
  }
  return { mode: 'compare', base: baseline, other, metrics };
}

const byVariant = (rows) => Map.groupBy(rows, (row) => row.variant ?? '');

/** Power-law growth across sizes: the log-log slope of the median per size. */
export function growthBySize(allRows) {
  const rows = allRows.filter((row) => row.warmup !== true);
  if (rows.some((row) => !(Number(row.size) > 0))) throw new Error('every row needs a "size" above 0');
  const names = metricNames(rows);
  if (!names.length) throw new Error('no numeric metric fields');
  const results = [];
  for (const [variant, group] of byVariant(rows)) {
    for (const name of names) {
      const points = group.filter((row) => Number.isFinite(row[name]));
      if (points.some((row) => row[name] <= 0)) throw new Error(`${name}: a log-log fit needs values above 0`);
      const sizes = [...Map.groupBy(points, (row) => row.size)].sort(([a], [b]) => a - b).map(([size, list]) => ({ size, ...describe(list.map((row) => row[name])) }));
      const line = sizes.length >= 2 ? fit(sizes.map(({ size, median: value }) => [Math.log(size), Math.log(value)])) : null;
      results.push({
        ...(variant ? { variant } : {}),
        metric: name,
        sizes,
        exponent: line ? round(line.slope, 2) : null,
        r2: line ? round(line.r2, 3) : null,
        nearestPower: line && sizes.length >= 3 ? Math.round(line.slope) : null,
      });
    }
  }
  return { mode: 'growth-size', results };
}

/** Linear growth across equivalent cycles: overall and later-half slopes against the later fit's residual range. */
export function growthByCycle(allRows) {
  const rows = allRows.filter((row) => row.warmup !== true);
  if (rows.some((row) => !Number.isFinite(row.cycle))) throw new Error('every row needs a numeric "cycle"');
  const names = metricNames(rows);
  if (!names.length) throw new Error('no numeric metric fields');
  const results = [];
  for (const [variant, group] of byVariant(rows)) {
    for (const name of names) {
      const points = group.filter((row) => Number.isFinite(row[name])).map((row) => [row.cycle, row[name]]);
      const cycles = [...new Set(points.map(([cycle]) => cycle))].sort((a, b) => a - b);
      const head = { ...(variant ? { variant } : {}), metric: name, cycles: cycles.length, first: points[0]?.[1] ?? null, last: points.at(-1)?.[1] ?? null };
      if (cycles.length < 6) {
        results.push({ ...head, verdict: 'insufficient-cycles' });
        continue;
      }
      const middle = median(cycles);
      const late = points.filter(([cycle]) => cycle >= middle);
      const all = fit(points);
      const later = fit(late);
      const lateGrowth = later.slope * (cycles.at(-1) - middle);
      const lateNoise = range(later.residuals);
      const totalGrowth = all.slope * (cycles.at(-1) - cycles[0]);
      const totalNoise = range(all.residuals);
      const verdict =
        lateGrowth > lateNoise ? 'growing' : lateGrowth < -lateNoise ? 'falling' : Math.abs(totalGrowth) > totalNoise ? 'plateau' : 'flat';
      results.push({
        ...head,
        perCycle: round(all.slope),
        latePerCycle: round(later.slope),
        lateGrowth: round(lateGrowth),
        lateResidualRange: round(lateNoise),
        totalGrowth: round(totalGrowth),
        totalResidualRange: round(totalNoise),
        verdict,
      });
    }
  }
  return { mode: 'growth-cycle', results };
}

/** Runs one command under /bin/sh; resolves { wallMs, cpuMs, exitCode, stdout, stderr }. */
export function runOnce(command, cwd) {
  // The subshell keeps an `exit` in the command from skipping `times`, whose second line is the children's CPU time.
  const script = `(\n${command}\n)\n__status=$?\ntimes >&3\nexit $__status\n`;
  return new Promise((resolve, reject) => {
    const started = performance.now();
    const child = spawn('/bin/sh', ['-c', script], { cwd, stdio: ['ignore', 'pipe', 'pipe', 'pipe'] });
    const chunks = [[], [], []];
    [child.stdout, child.stderr, child.stdio[3]].forEach((stream, i) => stream.on('data', (chunk) => chunks[i].push(chunk)));
    child.on('error', reject);
    child.on('close', (code, signal) => {
      const wallMs = performance.now() - started;
      const [stdout, stderr, times] = chunks.map((list) => Buffer.concat(list).toString('utf8'));
      const clock = [...times.matchAll(/(\d+)m([\d.]+)s/g)].map(([, minutes, seconds]) => Number(minutes) * 60 + Number(seconds));
      const cpuMs = clock.length >= 4 ? (clock[2] + clock[3]) * 1000 : null;
      resolve({ wallMs, cpuMs, exitCode: code ?? signal, stdout, stderr });
    });
  });
}

async function drive(options) {
  const rounds = [];
  const record = async (variant, index, warmup) => {
    let result;
    try {
      result = await runOnce(options[variant], options.cwd);
    } catch (error) {
      fail('command-failed', `${variant}: ${errorMessage(error)}`, 1, { rounds });
    }
    const row = { variant, round: index, ...(warmup ? { warmup: true } : {}), exitCode: result.exitCode, wallMs: round(result.wallMs), cpuMs: round(result.cpuMs) };
    if (options.parse) {
      const value = Number(options.parse.exec(result.stdout)?.[1]);
      if (!Number.isFinite(value)) fail('parse-missed', `${variant} round ${index}: --parse found no number in stdout`, 1, { rounds: [...rounds, row] });
      row.parsed = value;
    }
    rounds.push(row);
    if (result.exitCode !== 0) fail('command-failed', `${variant} round ${index} exited ${result.exitCode}: ${result.stderr.slice(-2000)}`, 1, { rounds });
  };
  for (let i = 1; i <= options.warmup; i++) for (const variant of ['old', 'new']) await record(variant, i, true);
  for (let i = 1; i <= options.rounds; i++) for (const variant of ['old', 'new']) await record(variant, i, false);
  return { ...compare(rounds, 'old'), rounds };
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.old !== undefined) {
    console.log(JSON.stringify({ ok: true, ...(await drive(options)) }));
    return;
  }
  let rows;
  try {
    rows = parseRows(readFileSync(options.file === '-' ? 0 : options.file, 'utf8'));
  } catch (error) {
    fail('invalid-input', `${options.file}: ${errorMessage(error)}`, 2);
  }
  try {
    const analysis = options.growth === 'size' ? growthBySize(rows) : options.growth === 'cycle' ? growthByCycle(rows) : compare(rows, options.base);
    console.log(JSON.stringify({ ok: true, ...analysis }));
  } catch (error) {
    fail('invalid-input', errorMessage(error), 2);
  }
}

if (isMainModule(import.meta.url)) await main();
