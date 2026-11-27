#!/usr/bin/env node
// Compares two coverage reports of the same code and lists, per file, the lines and branches that lost coverage.
// Built-ins only.
import { readFileSync } from 'node:fs';
import { isAbsolute, relative, resolve } from 'node:path';
import { errorMessage, fail } from './lib/fail.mjs';
import { isMainModule } from './lib/entrypoint.mjs';

const USAGE = `Usage: node coverage-diff.mjs <before> <after> [--before-root <dir>] [--after-root <dir>]

Compares the coverage report of the baseline (<before>) with the report after the change (<after>). Formats,
detected by content:
  lcov          lcov.info: SF/DA/BRDA records, LF/LH/BRF/BRH when a record has no DA/BRDA lines (c8, nyc, jest,
                vitest, cargo llvm-cov --lcov, coverage.py lcov); a file in several records is merged
  json-summary  istanbul coverage-summary.json (lines and branches per file)
  istanbul      istanbul coverage-final.json (statementMap/s for lines, b for branches)
  coverage.py   coverage json (files with summary num_statements/covered_lines/num_branches/covered_branches)
File paths are made relative to --before-root / --after-root (default: the current directory), so a baseline
measured in another checkout lines up.

A file loses coverage when it has more uncovered lines or more uncovered branches after than before. Deleting
covered code together with its tests is no loss; removing uncovered code is a gain. A file only in the after
report starts from zero uncovered; a file only in the before report is listed as removed and never loses. A moved
file therefore shows as removed plus lost: compare the two entries before calling it a loss. Branches are compared
only when both reports carry branch data.

Output (stdout, JSON): {"ok","before","after","branchesCompared","counts","lost","removed"}
  before, after  {"path","format","files","lines":{"covered","total"},"branches":{"covered","total"}}
  lost     [{"file","lines":{"before","after","lost"},"branches":{"before","after","lost"}}]: before/after are
           {"covered","total"} (before is null for a new file); lost = uncovered after minus uncovered before
  removed  [{"file","uncoveredLines","uncoveredBranches"}]
Reads the reports only; changes nothing.
Exit 0: no file lost coverage. 1: at least one file lost lines or branches. 2: invalid input (unreadable file,
unknown format, a report with no files): {"ok":false,"error":{"code","message"}}.`;

class InputError extends Error {
  constructor(code, message) {
    super(message);
    this.code = code;
  }
}

const counts = (covered, total) => ({ covered, total });
const uncovered = value => value.total - value.covered;

function relativeTo(root, file) {
  const text = String(file).replaceAll('\\', '/').replace(/^\.\//, '');
  if (!isAbsolute(text)) return text;
  const rel = relative(root, text);
  return (rel && !rel.startsWith('..') && !isAbsolute(rel) ? rel : text).replaceAll('\\', '/');
}

export function parseLcov(text, root) {
  const records = new Map();
  let current = null;
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (line.startsWith('SF:')) {
      const file = relativeTo(root, line.slice(3));
      current = records.get(file) ?? { lines: new Map(), branches: new Map(), lf: 0, lh: 0, brf: 0, brh: 0 };
      records.set(file, current);
      continue;
    }
    if (!current) continue;
    if (line === 'end_of_record') { current = null; continue; }
    const [key, value = ''] = [line.slice(0, line.indexOf(':')), line.slice(line.indexOf(':') + 1)];
    if (key === 'DA') {
      const [number, hits] = value.split(',');
      current.lines.set(number, Math.max(current.lines.get(number) ?? 0, Number(hits) || 0));
    } else if (key === 'BRDA') {
      const [number, block, branch, taken] = value.split(',');
      const id = `${number},${block},${branch}`;
      current.branches.set(id, Math.max(current.branches.get(id) ?? 0, taken === '-' ? 0 : Number(taken) || 0));
    } else if (key === 'LF' || key === 'LH' || key === 'BRF' || key === 'BRH') {
      current[key.toLowerCase()] = Math.max(current[key.toLowerCase()], Number(value) || 0);
    }
  }
  const files = new Map();
  for (const [file, record] of records) {
    const hits = [...record.lines.values()];
    const taken = [...record.branches.values()];
    files.set(file, {
      lines: hits.length ? counts(hits.filter(Boolean).length, hits.length) : counts(record.lh, record.lf),
      branches: taken.length ? counts(taken.filter(Boolean).length, taken.length) : counts(record.brh, record.brf),
    });
  }
  return files;
}

function summaryFiles(report, root) {
  const files = new Map();
  for (const [file, entry] of Object.entries(report)) {
    if (file === 'total') continue;
    files.set(relativeTo(root, file), {
      lines: counts(entry.lines?.covered ?? 0, entry.lines?.total ?? 0),
      branches: counts(entry.branches?.covered ?? 0, entry.branches?.total ?? 0),
    });
  }
  return files;
}

function istanbulFiles(report, root) {
  const files = new Map();
  for (const [key, entry] of Object.entries(report)) {
    const lines = new Map();
    for (const [id, location] of Object.entries(entry.statementMap ?? {})) {
      const line = location.start.line;
      lines.set(line, Math.max(lines.get(line) ?? 0, entry.s?.[id] ?? 0));
    }
    const branches = Object.values(entry.b ?? {}).flat();
    const hits = [...lines.values()];
    files.set(relativeTo(root, entry.path ?? key), {
      lines: counts(hits.filter(Boolean).length, hits.length),
      branches: counts(branches.filter(Boolean).length, branches.length),
    });
  }
  return files;
}

function coveragePyFiles(report, root) {
  const files = new Map();
  for (const [file, entry] of Object.entries(report.files)) {
    const summary = entry.summary ?? {};
    files.set(relativeTo(root, file), {
      lines: counts(summary.covered_lines ?? 0, summary.num_statements ?? 0),
      branches: counts(summary.covered_branches ?? 0, summary.num_branches ?? 0),
    });
  }
  return files;
}

/** Parses one report into file → {lines, branches} with {covered, total} counts. */
export function parseCoverage(text, root, where) {
  const start = text.trimStart();
  if (!start.startsWith('{')) {
    if (!/^SF:/m.test(text)) throw new InputError('unknown-format', `${where}: not lcov (no SF: record) and not JSON`);
    return { format: 'lcov', files: parseLcov(text, root) };
  }
  let report;
  try { report = JSON.parse(text); } catch (error) { throw new InputError('invalid-json', `${where}: ${errorMessage(error)}`); }
  if (report.files && typeof report.files === 'object' && report.meta) return { format: 'coverage.py', files: coveragePyFiles(report, root) };
  if (report.total?.lines && typeof report.total.lines.total === 'number') return { format: 'json-summary', files: summaryFiles(report, root) };
  const entries = Object.values(report);
  if (entries.length && entries.every(entry => entry && typeof entry === 'object' && 'statementMap' in entry)) return { format: 'istanbul', files: istanbulFiles(report, root) };
  throw new InputError('unknown-format', `${where}: JSON is not istanbul json-summary, istanbul coverage-final or coverage.py json`);
}

const totals = files => {
  const sum = key => [...files.values()].reduce((acc, file) => counts(acc.covered + file[key].covered, acc.total + file[key].total), counts(0, 0));
  return { lines: sum('lines'), branches: sum('branches') };
};

export function compareCoverage(before, after) {
  const branchesCompared = totals(before).branches.total > 0 && totals(after).branches.total > 0;
  const lost = [];
  const removed = [];
  const loss = (was, now) => Math.max(0, uncovered(now) - (was ? uncovered(was) : 0));
  for (const file of [...new Set([...before.keys(), ...after.keys()])].toSorted((a, b) => a.localeCompare(b))) {
    const was = before.get(file);
    const now = after.get(file);
    if (!now) {
      removed.push({ file, uncoveredLines: uncovered(was.lines), uncoveredBranches: uncovered(was.branches) });
      continue;
    }
    const lines = loss(was?.lines, now.lines);
    const branches = branchesCompared ? loss(was?.branches, now.branches) : 0;
    if (lines || branches) {
      lost.push({
        file,
        lines: { before: was?.lines ?? null, after: now.lines, lost: lines },
        branches: { before: was?.branches ?? null, after: now.branches, lost: branches },
      });
    }
  }
  return { branchesCompared, lost, removed };
}

function readReport(path, root) {
  let text;
  try { text = readFileSync(path, 'utf8'); } catch (error) { throw new InputError('unreadable', `${path}: ${error.code ?? errorMessage(error)}`); }
  const parsed = parseCoverage(text, root, path);
  if (!parsed.files.size) throw new InputError('no-files', `${path}: the ${parsed.format} report covers no files`);
  return parsed;
}

function parseArgs(argv) {
  const options = { paths: [], beforeRoot: process.cwd(), afterRoot: process.cwd() };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--help' || arg === '-h') { console.log(USAGE); process.exit(0); }
    if (!arg.startsWith('--')) { options.paths.push(arg); continue; }
    const value = argv[++i];
    if (value === undefined) fail('invalid-args', `${arg} needs a value`, 2);
    if (arg === '--before-root') options.beforeRoot = resolve(value);
    else if (arg === '--after-root') options.afterRoot = resolve(value);
    else fail('invalid-args', `unknown option ${arg}; see --help`, 2);
  }
  if (options.paths.length !== 2) fail('invalid-args', 'expected <before> <after>; see --help', 2);
  return options;
}

export function main(argv = process.argv.slice(2)) {
  const options = parseArgs(argv);
  const [beforePath, afterPath] = options.paths;
  let before;
  let after;
  try {
    before = readReport(beforePath, options.beforeRoot);
    after = readReport(afterPath, options.afterRoot);
  } catch (error) {
    if (error instanceof InputError) fail(error.code, error.message, 2);
    fail('read-failed', errorMessage(error), 2);
  }
  const result = compareCoverage(before.files, after.files);
  const side = (path, report) => ({ path, format: report.format, files: report.files.size, ...totals(report.files) });
  const sum = key => result.lost.reduce((acc, entry) => acc + entry[key].lost, 0);
  console.log(JSON.stringify({
    ok: result.lost.length === 0,
    before: side(beforePath, before),
    after: side(afterPath, after),
    branchesCompared: result.branchesCompared,
    counts: { lostFiles: result.lost.length, linesLost: sum('lines'), branchesLost: sum('branches'), removedFiles: result.removed.length },
    lost: result.lost,
    removed: result.removed,
  }, null, 2));
  process.exit(result.lost.length ? 1 : 0);
}

if (isMainModule(import.meta.url)) main();
