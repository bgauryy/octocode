#!/usr/bin/env node
// One file-length rule for every source file (.ts .tsx .mts .cts .js .mjs .cjs .rs): at most
// 400 physical lines. Files that were longer when the rule landed are listed in
// file-length-baseline.json (path → line count), which may only shrink:
//   - a file over the limit that is not in the baseline fails;
//   - a baselined file that grew past its recorded count fails;
//   - a baselined file now at or under the limit, or gone, is stale and fails.
// --write rewrites the baseline without adding or raising an entry (stale entries leave,
// shrunk files record their new count); it creates the file when it does not exist yet.
// Generated files are excluded by name; test inputs (fixtures) are data, not source.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { isMainModule } from './lib/entrypoint.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const BASELINE = path.join(path.dirname(fileURLToPath(import.meta.url)), 'file-length-baseline.json');
export const MAX_LINES = 400;
const SOURCE = /\.(?:[cm]?[jt]s|tsx|rs)$/;
const GENERATED = /\.generated\.|(?:^|\/)tool_types\.rs$/;
const DATA_DIRS = /(?:^|\/)(?:node_modules|fixtures|__fixtures__|seeded|\.yarn)\//;

export const isChecked = file => SOURCE.test(file) && !GENERATED.test(file) && !DATA_DIRS.test(file);

/** Physical lines: newline count, plus one for a last line without a trailing newline. */
export function physicalLines(text) {
  if (text.length === 0) return 0;
  let lines = 0;
  for (let i = text.indexOf('\n'); i !== -1; i = text.indexOf('\n', i + 1)) lines++;
  return text.endsWith('\n') ? lines : lines + 1;
}

/** Tracked and new (not ignored) source files with their line counts. */
function measure() {
  const files = execFileSync('git', ['ls-files', '--cached', '--others', '--exclude-standard', '-z'], {
    cwd: ROOT,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
  })
    .split('\0')
    .filter(file => file && isChecked(file) && fs.existsSync(path.join(ROOT, file)));
  return new Map(files.map(file => [file, physicalLines(fs.readFileSync(path.join(ROOT, file), 'utf8'))]));
}

/** Compares measured counts with the baseline. */
export function compare(counts, baseline) {
  const failures = [];
  for (const [file, lines] of counts) {
    if (lines <= MAX_LINES) continue;
    const recorded = baseline[file];
    if (recorded === undefined) failures.push(`${file}: ${lines} lines, over the ${MAX_LINES}-line limit; split it by responsibility`);
    else if (lines > recorded) failures.push(`${file}: grew to ${lines} lines (baseline ${recorded}); the baseline only shrinks`);
  }
  for (const [file, recorded] of Object.entries(baseline)) {
    const lines = counts.get(file);
    if (lines === undefined) failures.push(`${file}: baselined (${recorded}) but gone; remove the entry (--write)`);
    else if (lines <= MAX_LINES) failures.push(`${file}: ${lines} lines, within the limit; remove the entry (--write)`);
  }
  return failures;
}

/** The baseline after a shrink: every entry still over the limit, at min(recorded, current). */
export function shrink(counts, baseline) {
  return Object.fromEntries(
    Object.entries(baseline)
      .filter(([file]) => (counts.get(file) ?? 0) > MAX_LINES)
      .map(([file, recorded]) => [file, Math.min(recorded, counts.get(file))])
      .toSorted(([a], [b]) => a.localeCompare(b))
  );
}

export function initial(counts) {
  return Object.fromEntries([...counts].filter(([, lines]) => lines > MAX_LINES).toSorted(([a], [b]) => a.localeCompare(b)));
}

function main() {
  const counts = measure();
  const write = process.argv.includes('--write');
  const exists = fs.existsSync(BASELINE);
  if (!exists && !write) throw new Error(`${path.relative(ROOT, BASELINE)} is missing; create it with --write`);
  const baseline = exists ? JSON.parse(fs.readFileSync(BASELINE, 'utf8')) : initial(counts);
  const next = write ? shrink(counts, baseline) : baseline;
  if (write) fs.writeFileSync(BASELINE, `${JSON.stringify(next, null, 2)}\n`);
  const failures = compare(counts, next);
  for (const failure of failures) console.error(failure);
  console.log(`file length: ${counts.size} files checked, ${Object.keys(next).length} baselined over ${MAX_LINES} lines, ${failures.length} failures`);
  process.exit(failures.length ? 1 : 0);
}

if (isMainModule(import.meta.url)) main();
