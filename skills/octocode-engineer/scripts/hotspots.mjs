#!/usr/bin/env node
// Rank files by change frequency × a size or complexity proxy, from read-only git history and the working tree.
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { isMainModule } from './lib/entrypoint.mjs';
import { errorMessage, fail } from './lib/fail.mjs';

const HELP = `Rank hotspots: files that change often and carry the most code. Runs read-only git commands; writes nothing.

Usage:
  node scripts/hotspots.mjs [--since <date>] [--complexity indent|lines] [--top <n>] [pathspec ...]
  node scripts/hotspots.mjs --help

Run inside the repository. Pathspecs limit the files (default: the whole repository); git exclusions work,
such as ':(exclude)vendor'.
  --since <date>        Count commits after this date: YYYY-MM-DD, an ISO date-time, or "<n> days|weeks|months|years ago".
                        Default: the whole history.
  --complexity <proxy>  indent (default) or lines.
  --top <n>             Keep the n highest scores in files (summary still counts every file).

Formula (also stated in the output, so a later run can be compared only when it matches):
  score       = commits × complexity
  commits     = non-merge commits in the window that changed the file (git log --no-merges --no-renames)
  complexity  indent: Σ over non-blank lines of (leading tabs + ⌊leading spaces ÷ unit⌋), where unit is the
                      file's most frequent positive indentation step between consecutive non-blank lines (4 when none)
              lines:  non-blank lines
  churn       = lines added + deleted in the window: reported, not scored
  Files: changed in the window and present in the working tree now. A rename starts a new history (the old path
  no longer exists, so its commits are dropped). Binary files are skipped.

Output (JSON on stdout):
  ok          true
  formula     {score, commits, complexity}: the exact calculation used
  window      {since, head}: the window start (null: whole history) and the HEAD commit read
  paths       the pathspecs given
  files       [{path, score, commits, complexity, lines, indentUnit, added, deleted}] by score, then commits, then path
  summary     {commits, filesChanged, filesRanked, skipped: {binary, notInWorkingTree}}

Exit codes: 0 ranked (files may be empty); 2 invalid input: a bad option, --since or --top value, or not inside a
git work tree ({ok:false, error:{code, message}}); 1 a git command failed.
`;

const RELATIVE_DATE = /^\d+ (?:day|week|month|year)s? ago$/;
const ISO_DATE = /^\d{4}-\d{2}-\d{2}(?:[T ]\d{2}:\d{2}(?::\d{2})?(?:\.\d+)?(?:Z|[+-]\d{2}:?\d{2})?)?$/;

class InputError extends Error {
  constructor(code, message) {
    super(message);
    this.code = code;
  }
}

class GitError extends Error {}

function git(cwd, args) {
  const run = spawnSync('git', args, { cwd, encoding: 'utf8', maxBuffer: 1 << 30 });
  if (run.error) throw new GitError(`git ${args[0]}: ${run.error.message}`);
  if (run.status !== 0) throw new GitError(`git ${args[0]} exited ${run.status}: ${run.stderr.trim()}`);
  return run.stdout;
}

export function parseArgs(argv) {
  const options = { since: null, complexity: 'indent', top: null, paths: [], help: false };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--help' || arg === '-h') options.help = true;
    else if (arg === '--since' || arg === '--complexity' || arg === '--top') {
      const value = argv[++i];
      if (value === undefined || value.startsWith('--')) throw new InputError('invalidArguments', `Missing value for ${arg}`);
      if (arg === '--since') {
        if (!RELATIVE_DATE.test(value) && !(ISO_DATE.test(value) && !Number.isNaN(Date.parse(value)))) {
          throw new InputError('invalidSince', `--since takes YYYY-MM-DD, an ISO date-time or "<n> days|weeks|months|years ago", got ${value}`);
        }
        options.since = value;
      } else if (arg === '--complexity') {
        if (value !== 'indent' && value !== 'lines') throw new InputError('invalidComplexity', `--complexity takes indent or lines, got ${value}`);
        options.complexity = value;
      } else {
        if (!/^[1-9]\d*$/.test(value)) throw new InputError('invalidTop', `--top takes a positive integer, got ${value}`);
        options.top = Number(value);
      }
    } else if (arg === '--') {
      options.paths.push(...argv.slice(i + 1));
      break;
    } else if (arg.startsWith('-')) throw new InputError('invalidArguments', `Unknown option: ${arg}`);
    else options.paths.push(arg);
  }
  return options;
}

/** Commit and churn counts per repository-relative path from `git log --numstat -z` output. */
export function parseLog(output) {
  const files = new Map();
  let commits = 0;
  for (const chunk of output.split('\x01').slice(1)) {
    commits++;
    const [, ...entries] = chunk.split('\0');
    for (const raw of entries) {
      const entry = raw.replace(/^\n/, '');
      if (!entry) continue;
      const [added, deleted, ...rest] = entry.split('\t');
      const path = rest.join('\t');
      const stats = files.get(path) ?? { commits: 0, added: 0, deleted: 0, binary: false };
      stats.commits++;
      if (added === '-' || deleted === '-') stats.binary = true;
      else {
        stats.added += Number(added);
        stats.deleted += Number(deleted);
      }
      files.set(path, stats);
    }
  }
  return { commits, files };
}

/** Size and indentation measures of file text (see the formula in --help). */
export function measure(text) {
  const lines = text.split(/\r?\n/).filter((line) => line.trim());
  const indents = lines.map((line) => {
    const lead = /^[\t ]*/.exec(line)[0];
    const tabs = lead.length - lead.replaceAll('\t', '').length;
    return { tabs, spaces: lead.length - tabs };
  });
  const steps = new Map();
  for (let i = 1; i < indents.length; i++) {
    const step = indents[i].spaces - indents[i - 1].spaces;
    if (step > 0) steps.set(step, (steps.get(step) ?? 0) + 1);
  }
  let unit = 4;
  let best = 0;
  for (const [step, count] of [...steps].sort((a, b) => a[0] - b[0])) {
    if (count > best) [unit, best] = [step, count];
  }
  const indent = indents.reduce((sum, { tabs, spaces }) => sum + tabs + Math.floor(spaces / unit), 0);
  return { lines: lines.length, indent, indentUnit: unit };
}

export function hotspots(cwd, options) {
  let root;
  try {
    root = git(cwd, ['rev-parse', '--show-toplevel']).trim();
  } catch (error) {
    throw new InputError('notAGitRepository', `Not inside a git work tree: ${cwd} (${errorMessage(error)})`);
  }
  const head = git(cwd, ['rev-parse', '--verify', '--quiet', 'HEAD']).trim();
  const specs = options.paths.length ? options.paths : ['.'];
  const tracked = new Set(git(cwd, ['ls-files', '-z', '--full-name', '--', ...specs]).split('\0').filter(Boolean));
  const log = parseLog(git(cwd, [
    'log', '--no-merges', '--no-renames', '--numstat', '-z', '--format=%x01%H',
    ...(options.since ? [`--since=${options.since}`] : []), '--', ...specs,
  ]));
  const skipped = { binary: 0, notInWorkingTree: 0 };
  const files = [];
  for (const [path, stats] of log.files) {
    if (stats.binary) {
      skipped.binary++;
      continue;
    }
    let text;
    try {
      if (!tracked.has(path)) throw new Error('untracked');
      text = readFileSync(join(root, path), 'utf8');
    } catch {
      skipped.notInWorkingTree++;
      continue;
    }
    if (text.includes('\0')) {
      skipped.binary++;
      continue;
    }
    const size = measure(text);
    const complexity = options.complexity === 'lines' ? size.lines : size.indent;
    files.push({ path, score: stats.commits * complexity, commits: stats.commits, complexity, lines: size.lines, indentUnit: size.indentUnit, added: stats.added, deleted: stats.deleted });
  }
  files.sort((a, b) => b.score - a.score || b.commits - a.commits || a.path.localeCompare(b.path));
  return {
    ok: true,
    formula: {
      score: 'commits × complexity',
      commits: 'non-merge commits in the window that changed the file (git log --no-merges --no-renames)',
      complexity: options.complexity === 'lines'
        ? 'lines: non-blank lines'
        : 'indent: Σ over non-blank lines of (leading tabs + ⌊leading spaces ÷ indentUnit⌋); indentUnit = the most frequent positive indentation step (4 when none)',
    },
    window: { since: options.since, head },
    paths: options.paths,
    files: options.top ? files.slice(0, options.top) : files,
    summary: { commits: log.commits, filesChanged: log.files.size, filesRanked: files.length, skipped },
  };
}

function main(argv) {
  let report;
  try {
    const options = parseArgs(argv);
    if (options.help) {
      console.log(HELP);
      return;
    }
    report = hotspots(process.cwd(), options);
  } catch (error) {
    if (error instanceof InputError) fail(error.code, error.message, 2);
    fail(error instanceof GitError ? 'gitFailed' : 'hotspotsFailed', errorMessage(error), 1);
  }
  console.log(JSON.stringify(report, null, 2));
}

if (isMainModule(import.meta.url)) main(process.argv.slice(2));
