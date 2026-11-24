#!/usr/bin/env node
// Capacity models fitted to measured load-test data. usl: the Universal Scalability Law.
import { readFileSync } from 'node:fs';
import { isMainModule } from './lib/entrypoint.mjs';
import { errorMessage, fail } from './lib/fail.mjs';

const HELP = `Fit a capacity model to load-test measurements. Reads only; writes nothing.

Usage:
  node scripts/capacity.mjs usl <runs.csv | ->
  node scripts/capacity.mjs --help

usl: the Universal Scalability Law, X(N) = λN / (1 + α(N − 1) + βN(N − 1)).
  Input CSV: concurrency,throughput per row (one row per measured run; repeated concurrency levels are kept),
  an optional header row, # comments and blank lines skipped. "-" reads stdin. At least three distinct
  concurrency levels; concurrency ≥ 1 and throughput > 0, in one consistent unit.
  Fit: weighted least squares on N/X = 1/λ + (α/λ)(N − 1) + (β/λ)N(N − 1), each row weighted (X²/N)² so the
  fit approximates least squares on throughput; α and β are held at 0 when the unconstrained fit makes them
  negative (the model with the lower throughput error among the feasible ones is kept).

Output (JSON on stdout):
  ok            true
  model         "usl"; formula: the model above
  lambda        λ: throughput of one unit of concurrency without contention
  alpha, beta   α contention (serialized share), β coherency (crosstalk) penalties
  constrained   parameters held at 0 by the non-negativity rule ([] when none)
  peak          {concurrency: √((1 − α)/β), bestInteger, throughput, extrapolated} when β > 0;
                null when β = 0 (no retrograde region: throughput approaches λ/α, or grows linearly when α = 0)
  ceiling       λ/α when β = 0 and α > 0, else null
  fit           {points, levels, dof, r2, rmse, maxRelativeError}: r2 and errors on throughput, dof = points − 3
  rows          [{concurrency, throughput, predicted, relativeError}] in input order
  warnings      [text]: a parameter held at 0, a peak beyond the largest measured concurrency, α ≥ 1, or an
                exact fit (dof 0)

Exit codes: 0 fitted; 2 invalid input: unknown model, unreadable file, a malformed or non-positive row, or fewer
than three concurrency levels ({ok:false, error:{code, message}}); 1 the fit failed (singular data).
`;

class InputError extends Error {
  constructor(code, message) {
    super(message);
    this.code = code;
  }
}

/** Measured rows from CSV text: [{concurrency, throughput, line}]. */
export function parseRuns(text) {
  const rows = [];
  text.split(/\r?\n/).forEach((raw, index) => {
    const line = raw.trim();
    if (!line || line.startsWith('#')) return;
    const cells = line.split(',').map((cell) => cell.trim());
    const numeric = cells.length === 2 && cells.every((cell) => cell !== '' && Number.isFinite(Number(cell)));
    if (!numeric) {
      if (!rows.length && cells.length === 2 && cells.every((cell) => /[A-Za-z]/.test(cell))) return; // header
      throw new InputError('invalidRow', `Line ${index + 1}: expected concurrency,throughput numbers, got "${line}"`);
    }
    const [concurrency, throughput] = cells.map(Number);
    if (concurrency < 1 || throughput <= 0) throw new InputError('invalidRow', `Line ${index + 1}: concurrency must be ≥ 1 and throughput > 0, got "${line}"`);
    rows.push({ concurrency, throughput, line: index + 1 });
  });
  const levels = new Set(rows.map((row) => row.concurrency)).size;
  if (levels < 3) throw new InputError('tooFewLevels', `USL has three parameters: measure at least three concurrency levels (got ${levels})`);
  return rows;
}

/** Solve a small linear system by Gaussian elimination with partial pivoting; null when singular. */
function solve(matrix, vector) {
  const n = vector.length;
  const a = matrix.map((row, i) => [...row, vector[i]]);
  for (let col = 0; col < n; col++) {
    let pivot = col;
    for (let row = col + 1; row < n; row++) if (Math.abs(a[row][col]) > Math.abs(a[pivot][col])) pivot = row;
    const scale = Math.max(...a.map((row) => Math.abs(row[col])));
    if (!scale || Math.abs(a[pivot][col]) <= scale * 1e-12) return null;
    [a[col], a[pivot]] = [a[pivot], a[col]];
    for (let row = 0; row < n; row++) {
      if (row === col) continue;
      const factor = a[row][col] / a[col][col];
      for (let k = col; k <= n; k++) a[row][k] -= factor * a[col][k];
    }
  }
  return a.map((row, i) => row[n] / row[i]);
}

const predict = ({ lambda, alpha, beta }, n) => (lambda * n) / (1 + alpha * (n - 1) + beta * n * (n - 1));

/** Weighted least squares for the given free terms of N/X = a + b(N − 1) + c·N(N − 1). */
function fitTerms(rows, free) {
  const basis = { a: () => 1, b: (n) => n - 1, c: (n) => n * (n - 1) };
  const weights = rows.map(({ concurrency: n, throughput: x }) => (x * x / n) ** 2);
  const top = Math.max(...weights);
  const normal = free.map(() => free.map(() => 0));
  const rhs = free.map(() => 0);
  rows.forEach(({ concurrency: n, throughput: x }, i) => {
    const w = weights[i] / top;
    const y = n / x;
    free.forEach((p, r) => {
      rhs[r] += w * basis[p](n) * y;
      free.forEach((q, c) => (normal[r][c] += w * basis[p](n) * basis[q](n)));
    });
  });
  const solution = solve(normal, rhs);
  if (!solution) return null;
  const terms = { a: 0, b: 0, c: 0 };
  free.forEach((p, i) => (terms[p] = solution[i]));
  // A term negative only by rounding (its largest contribution under 1e-9 of 1/λ) is zero, not a constraint.
  const maxN = Math.max(...rows.map((row) => row.concurrency));
  for (const [p, scale] of [['b', maxN - 1], ['c', maxN * (maxN - 1)]]) {
    if (terms[p] < 0 && -terms[p] * scale <= 1e-9 * Math.abs(terms.a)) terms[p] = 0;
  }
  if (terms.a <= 0 || terms.b < 0 || terms.c < 0) return null;
  const params = { lambda: 1 / terms.a, alpha: terms.b / terms.a, beta: terms.c / terms.a };
  const error = rows.reduce((sum, { concurrency: n, throughput: x }) => sum + (x - predict(params, n)) ** 2, 0);
  return { params, error, constrained: ['alpha', 'beta'].filter((name, i) => !free.includes('bc'[i])) };
}

/** USL fit of measured rows (see --help for the method and output). */
export function fitUsl(rows) {
  const full = fitTerms(rows, ['a', 'b', 'c']);
  const candidates = full ? [full] : [['a', 'b'], ['a', 'c'], ['a']].map((free) => fitTerms(rows, free)).filter(Boolean);
  if (!candidates.length) throw new Error('No non-negative USL fit exists for these rows (the normal equations are singular)');
  const { params, constrained } = candidates.reduce((best, next) => (next.error < best.error ? next : best));
  const { lambda, alpha, beta } = params;
  const throughputs = rows.map((row) => row.throughput);
  const mean = throughputs.reduce((sum, x) => sum + x, 0) / rows.length;
  const fitted = rows.map(({ concurrency, throughput }) => {
    const predicted = predict(params, concurrency);
    return { concurrency, throughput, predicted, relativeError: (predicted - throughput) / throughput };
  });
  const residual = fitted.reduce((sum, row) => sum + (row.throughput - row.predicted) ** 2, 0);
  const total = throughputs.reduce((sum, x) => sum + (x - mean) ** 2, 0);
  const maxMeasured = Math.max(...rows.map((row) => row.concurrency));
  const levels = new Set(rows.map((row) => row.concurrency)).size;
  const warnings = [];
  let peak = null;
  if (beta > 0) {
    const concurrency = alpha < 1 ? Math.sqrt((1 - alpha) / beta) : 1;
    const bestInteger = [Math.max(1, Math.floor(concurrency)), Math.max(1, Math.ceil(concurrency))]
      .reduce((best, n) => (predict(params, n) > predict(params, best) ? n : best));
    peak = { concurrency, bestInteger, throughput: predict(params, concurrency), extrapolated: concurrency > maxMeasured };
    if (peak.extrapolated) warnings.push(`The peak (N ≈ ${concurrency.toFixed(1)}) lies beyond the largest measured concurrency (${maxMeasured}): measure near it before relying on it`);
  }
  if (constrained.length) warnings.push(`${constrained.join(' and ')} held at 0: the unconstrained fit was negative (superlinear scaling, warm-up or noise)`);
  if (alpha >= 1) warnings.push('α ≥ 1: throughput falls from the first unit of concurrency; check the units and the N = 1 measurement');
  if (rows.length - 3 <= 0) warnings.push('dof 0: three points fit three parameters exactly, so r2 = 1 proves nothing; add measured levels');
  return {
    ok: true,
    model: 'usl',
    formula: 'X(N) = λN / (1 + α(N − 1) + βN(N − 1))',
    lambda,
    alpha,
    beta,
    constrained,
    peak,
    ceiling: beta === 0 && alpha > 0 ? lambda / alpha : null,
    fit: {
      points: rows.length,
      levels,
      dof: rows.length - 3,
      r2: total > 0 ? 1 - residual / total : null,
      rmse: Math.sqrt(residual / rows.length),
      maxRelativeError: Math.max(...fitted.map((row) => Math.abs(row.relativeError))),
    },
    rows: fitted,
    warnings,
  };
}

function main(argv) {
  if (argv.includes('--help') || argv.includes('-h')) {
    console.log(HELP);
    return;
  }
  let report;
  try {
    const [model, file, ...extra] = argv;
    if (model !== 'usl') throw new InputError('unknownModel', `Unknown or missing model ${model ?? ''}: use usl; see --help`);
    if (!file || extra.length) throw new InputError('invalidArguments', 'Pass one CSV file (or - for stdin): capacity.mjs usl <runs.csv>');
    let text;
    try {
      text = readFileSync(file === '-' ? 0 : file, 'utf8');
    } catch (error) {
      throw new InputError('unreadable', `${file}: ${errorMessage(error)}`);
    }
    report = fitUsl(parseRuns(text));
  } catch (error) {
    if (error instanceof InputError) fail(error.code, error.message, 2);
    fail('fitFailed', errorMessage(error), 1);
  }
  console.log(JSON.stringify(report, null, 2));
}

if (isMainModule(import.meta.url)) main(process.argv.slice(2));
