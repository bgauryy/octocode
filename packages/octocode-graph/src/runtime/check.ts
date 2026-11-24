// `octocode-graph check` (CLI and MCP check_graph): a rules file (rules.ts) over a saved graph, a baseline of known
// violations, and the answer as text, JSON or SARIF 2.1.0 (sarif.ts). Exit 1 when an `error` violation the baseline does
// not list is reported, 0 otherwise; a rules file or argument that cannot be used is exit 2 (CheckError).
//
// `--since <rev>` is the pull-request overlay: git (read-only: rev-parse, merge-base, diff, ls-files) lists the files
// changed since the merge base of <rev> and HEAD, plus uncommitted and untracked ones. The saved graph refreshes its
// changed chunks (api.ts refreshGraph). Its base is the saved graph of the merge base: one an earlier `check --since`
// kept (since-base.json) for that commit, else the graph as it was before this refresh, else the graph its previous
// refresh replaced (previous.json), each only when the files it differs in from the working tree are exactly the
// graph files the diff lists. Reported are the violations the base does not have whose source changed or whose chain
// crosses a changed file. No base: the answer says so and reports every violation.
import { execFile } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  writeFileSync,
} from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { promisify } from 'node:util';
import { refreshGraph, type RefreshSummary } from './api.ts';
import { readEvents } from './pipeline.ts';
import { graphPath, resultDir } from './query.ts';
import type { QueryData } from './query-index.ts';
import { editedSince } from './query.ts';
import { hopsText } from './query-reach.ts';
import { plural } from './query-ops.ts';
import { registeredRuns } from './registry.ts';
import {
  applyBaseline,
  baselineOf,
  checkRules,
  NOT_IN_ALLOWED,
  parseBaseline,
  parseRules,
  RulesError,
  violationKey,
  type RuleSet,
  type Violation,
} from './rules.ts';
import { sarifLog, type RuleInfo } from './sarif.ts';
import { rootOf } from '../ids.ts';

export const FORMATS = ['text', 'json', 'sarif'] as const;
export type CheckFormat = (typeof FORMATS)[number];

export interface CheckOptions {
  /** A result directory, its data.json, or a project directory (its newest saved graph); default: `.`. */
  graph?: string;
  /** Default: .octocode/graph-rules.json at the graph root. */
  rules?: string;
  /** Default: graph-rules.baseline.json beside the rules file, when it exists. */
  baseline?: string;
  /** Record every current violation as known in the baseline file. */
  writeBaseline?: boolean;
  /** Report only what changed since this git revision (the pull-request overlay). */
  since?: string;
  format?: CheckFormat;
  /** Progress and warnings meant for stderr. */
  onNote?: (text: string) => void;
}

/** A check that cannot run as asked: bad arguments, no or invalid rules or baseline, no graph, a failing git. */
export class CheckError extends Error {
  readonly candidates: string[];
  constructor(message: string, candidates: string[] = []) {
    super(message);
    this.candidates = candidates;
  }
}

/** What `--since` compared against and what it left out. */
export interface SinceReport {
  rev: string;
  commit: string;
  mergeBase: string;
  /** Files changed since the merge base (committed, uncommitted and untracked), relative to the graph root. */
  changed: string[];
  /** Chunks the refresh rebuilt. */
  rebuilt: string[];
  /** Which saved graph served as the base; null: none matched the merge base, so every violation is reported. */
  base: string | null;
  /** Violations the base graph has too: not introduced by the change. */
  preExisting: number;
  /** New violations whose chain crosses no changed file. */
  outside: number;
  note: string;
}

export interface CheckReport {
  ok: true;
  failed: boolean;
  graph: string;
  rules: string;
  baseline: string | null;
  /** Reported violations; `known: true` on those the baseline lists. */
  violations: Violation[];
  counts: {
    error: number;
    warn: number;
    info: number;
    known: number;
    fixed: number;
  };
  /** Baseline entries no longer violated: rewrite the baseline to drop them. */
  fixed: { rule: string; from: string; to: string }[];
  since?: SinceReport;
  summary: string;
}

export interface CheckOutcome {
  /** What to print: the text, JSON or SARIF answer, or the baseline written. */
  output: string;
  exitCode: 0 | 1;
  report: CheckReport;
}

const readJson = (path: string, what: string): unknown => {
  try {
    return JSON.parse(readFileSync(path, 'utf8'));
  } catch (error) {
    throw new CheckError(
      `${what} ${path} is not readable JSON: ${error instanceof Error ? error.message : String(error)}`
    );
  }
};
const writeJson = (path: string, value: unknown) => {
  mkdirSync(dirname(path), { recursive: true });
  const temp = `${path}.${process.pid}.tmp`;
  writeFileSync(temp, `${JSON.stringify(value, null, 2)}\n`);
  renameSync(temp, path);
};

/** The rules a report and a SARIF log describe: every named rule, and not-in-allowed when allowed rules exist. */
function ruleInfos(set: RuleSet): RuleInfo[] {
  const byId = new Map<string, RuleInfo>();
  for (const r of set.rules)
    if (!byId.has(r.name))
      byId.set(r.name, {
        id: r.name,
        severity: r.severity,
        type: r.type,
        ...(r.type !== 'allowed' && r.comment && { comment: r.comment }),
      });
  const allowed = byId.get(NOT_IN_ALLOWED);
  if (allowed) allowed.comment = 'A dependency that no allowed rule matches';
  return [...byId.values()];
}

/** Evaluate a rules file over a saved graph; throws CheckError for anything that keeps it from running. */
export async function runCheck(
  options: CheckOptions = {}
): Promise<CheckOutcome> {
  const format = options.format ?? 'text';
  if (!FORMATS.includes(format))
    throw new CheckError(`--format takes ${FORMATS.join(', ')}, not ${format}`);
  if (options.since !== undefined && !options.since.trim())
    throw new CheckError(
      '--since needs a git revision (a branch, tag or commit)'
    );
  if (options.since && options.writeBaseline)
    throw new CheckError(
      '--write-baseline records the whole graph; run it without --since'
    );
  const note = options.onNote ?? (() => {});
  const arg = options.graph ?? '.';
  let path: string;
  let out: string | null;
  try {
    if (options.since) {
      out = resultDir(arg);
      path = join(out, 'data.json');
    } else {
      const found = graphPath(arg);
      path = found.path;
      out = found.chosen;
    }
  } catch (error) {
    throw new CheckError(
      error instanceof Error ? error.message : String(error)
    );
  }
  if (!existsSync(path)) throw new CheckError(`no saved graph at ${path}`);
  if (out && out !== dirname(path)) note(`checking ${out}`);
  let D = readJson(path, 'graph') as QueryData;
  const root = rootOf(D);
  if (!root && !options.rules)
    throw new CheckError(
      `${path} names no root (section): pass --rules <file>`
    );

  const rulesPath = resolve(
    options.rules ?? join(root, '.octocode', 'graph-rules.json')
  );
  if (!existsSync(rulesPath))
    throw new CheckError(
      `no rules file at ${rulesPath}: write one (skills/octocode-graph/references/rules.md) or pass --rules <file>`
    );
  const set = parse(
    () => parseRules(readJson(rulesPath, 'rules file')),
    rulesPath
  );
  const baselinePath = resolve(
    options.baseline ?? join(dirname(rulesPath), 'graph-rules.baseline.json')
  );
  const hasBaseline = existsSync(baselinePath);
  if (options.baseline && !hasBaseline && !options.writeBaseline)
    throw new CheckError(`no baseline file at ${baselinePath}`);
  const baseline =
    hasBaseline && !options.writeBaseline
      ? parse(
          () => parseBaseline(readJson(baselinePath, 'baseline')),
          baselinePath
        )
      : null;

  let since: Omit<SinceReport, 'preExisting' | 'outside'> | null = null;
  let base: QueryData | null = null;
  if (options.since) {
    const o = await overlay(out!, options.since, note);
    D = o.after;
    base = o.base;
    since = o.report;
  } else {
    const edited = editedSince(D, path);
    if (edited.length)
      note(
        `${plural(edited.length, 'graph file')} changed or missing since the graph was written (first: ${edited[0]}): rules are checked on the graph as written; refresh it first`
      );
  }

  const all = parse(() => checkRules(D, set), rulesPath);
  if (options.writeBaseline) {
    const written = baselineOf(all);
    writeJson(baselinePath, written);
    const summary = `wrote ${plural(written.violations.length, 'known violation')} to ${baselinePath}; later checks report them as known and fail only on new ones`;
    const report = reportOf(
      path,
      rulesPath,
      baselinePath,
      all,
      [],
      null,
      summary
    );
    return {
      output: format === 'text' ? summary : JSON.stringify(report),
      exitCode: 0,
      report,
    };
  }
  const { violations: marked, fixed } = applyBaseline(all, baseline);

  let reported = marked;
  let sinceReport: SinceReport | undefined;
  if (since) {
    let preExisting = 0,
      outside = 0;
    if (base) {
      const before = new Set(
        checkRules(base, set, { strict: false }).map(violationKey)
      );
      const changed = new Set(since.changed);
      const crosses = (v: Violation) =>
        changed.has(v.from) ||
        v.chain.some(h => changed.has(h.id)) ||
        Boolean(v.cycle?.some(id => changed.has(id)));
      const fresh = marked.filter(v => !before.has(violationKey(v)));
      reported = fresh.filter(crosses);
      preExisting = marked.length - fresh.length;
      outside = fresh.length - reported.length;
    }
    sinceReport = { ...since, preExisting, outside };
  }

  const counts = {
    error: reported.filter(v => !v.known && v.severity === 'error').length,
    warn: reported.filter(v => !v.known && v.severity === 'warn').length,
    info: reported.filter(v => !v.known && v.severity === 'info').length,
    known: reported.filter(v => v.known).length,
    fixed: fixed.length,
  };
  const rules = ruleInfos(set);
  const summary = [
    counts.error + counts.warn + counts.info
      ? `${plural(counts.error, 'error')}, ${plural(counts.warn, 'warning')}, ${counts.info} info`
      : 'no violations',
    counts.known ? ` (${counts.known} known: the baseline lists them)` : '',
    ` from ${plural(rules.length, 'rule')} over ${plural((D.files ?? []).length, 'file')}`,
    sinceReport ? `; ${sinceReport.note}` : '',
    fixed.length
      ? `; ${plural(fixed.length, 'baseline entry')} no longer violated (--write-baseline drops ${fixed.length === 1 ? 'it' : 'them'})`
      : '',
  ].join('');
  const report = reportOf(
    path,
    rulesPath,
    baseline ? baselinePath : null,
    reported,
    fixed,
    sinceReport ?? null,
    summary
  );
  report.counts = counts;
  report.failed = counts.error > 0;
  const output =
    format === 'json'
      ? JSON.stringify(report)
      : format === 'sarif'
        ? JSON.stringify(
            sarifLog(reported, rules, {
              root,
              isFile: id =>
                !id.startsWith('pkg:') ||
                (D.files ?? []).some(f => f.id === id),
            }),
            null,
            2
          )
        : formatCheck(report);
  return { output, exitCode: report.failed ? 1 : 0, report };
}

function parse<T>(read: () => T, file: string): T {
  try {
    return read();
  } catch (error) {
    if (error instanceof RulesError)
      throw new CheckError(
        `${file}:\n  ${error.problems.join('\n  ')}`,
        error.candidates
      );
    throw error;
  }
}

function reportOf(
  graph: string,
  rules: string,
  baseline: string | null,
  violations: Violation[],
  fixed: CheckReport['fixed'],
  since: SinceReport | null,
  summary: string
): CheckReport {
  return {
    ok: true,
    failed: false,
    graph,
    rules,
    baseline,
    violations,
    counts: { error: 0, warn: 0, info: 0, known: 0, fixed: fixed.length },
    fixed,
    ...(since && { since }),
    summary,
  };
}

/** The text answer: each reported violation with its chain and comment, then the summary. */
export function formatCheck(r: CheckReport) {
  const lines: string[] = [];
  for (const v of r.violations) {
    if (v.known) continue;
    lines.push(
      `${v.severity.padEnd(5)}  ${v.rule}  ${v.from} → ${v.to} (${v.kind})`
    );
    if (v.chain.length > 1) lines.push(`  ${hopsText(v.chain)}`);
    if (v.comment) lines.push(`  ${v.comment}`);
  }
  for (const f of r.fixed)
    lines.push(
      `fixed  ${f.rule}  ${f.from} → ${f.to} (listed in the baseline, no longer violated)`
    );
  lines.push(r.summary);
  return lines.join('\n');
}

// git, read only: never a lock, an index write or any other change to the repository.
const run = promisify(execFile);
async function git(root: string, args: string[]) {
  try {
    const { stdout } = await run('git', ['-C', root, ...args], {
      encoding: 'utf8',
      maxBuffer: 256 * 1024 * 1024,
      env: { ...process.env, GIT_OPTIONAL_LOCKS: '0' },
    });
    return stdout;
  } catch (error) {
    const detail =
      (error as { stderr?: string }).stderr?.trim() ||
      (error instanceof Error ? error.message : String(error));
    throw new CheckError(`git ${args.join(' ')} failed in ${root}: ${detail}`);
  }
}
const nulList = (text: string) => text.split('\0').filter(Boolean);

/** The files changed since the merge base of `rev` and HEAD, uncommitted and untracked ones too (root-relative). */
export async function changedSince(root: string, rev: string) {
  const commit = (
    await git(root, [
      'rev-parse',
      '--verify',
      '--quiet',
      `${rev}^{commit}`,
    ]).catch(() => {
      throw new CheckError(
        `--since ${rev} names no commit in the repository at ${root}`
      );
    })
  ).trim();
  if (!commit)
    throw new CheckError(
      `--since ${rev} names no commit in the repository at ${root}`
    );
  const mergeBase = (await git(root, ['merge-base', commit, 'HEAD'])).trim();
  const lists = await Promise.all([
    git(root, [
      'diff',
      '--name-only',
      '--no-renames',
      '--relative',
      '-z',
      `${rev}...HEAD`,
    ]),
    git(root, [
      'diff',
      '--name-only',
      '--no-renames',
      '--relative',
      '-z',
      'HEAD',
    ]),
    git(root, ['ls-files', '--others', '--exclude-standard', '-z']),
  ]);
  return {
    commit,
    mergeBase,
    changed: [...new Set(lists.flatMap(nulList))].sort(),
  };
}

/** The base graph an earlier `check --since` matched to a merge base, kept in the result directory. */
const SINCE_BASE = 'since-base.json';
interface KeptBase {
  mergeBase: string;
  graph: QueryData;
}

const graphFiles = (D: QueryData) =>
  (D.files ?? [])
    .filter(f => f.kind !== 'package' && !f.id.startsWith('pkg:'))
    .map(f => f.id);
const filesOf = (e: Partial<RefreshSummary> | undefined) => [
  ...(e?.changed ?? []),
  ...(e?.added ?? []),
  ...(e?.removed ?? []),
];

async function overlay(out: string, rev: string, note: (text: string) => void) {
  const dataPath = join(out, 'data.json');
  const before = readJson(dataPath, 'graph') as QueryData;
  const root = rootOf(before);
  if (!root)
    throw new CheckError(
      `${dataPath} names no root (section) to diff against ${rev}`
    );
  const { commit, mergeBase, changed } = await changedSince(root, rev);
  // Read before the refresh replaces them: previous.json and the refresh that wrote it.
  const previousPath = join(out, 'previous.json');
  const previous = existsSync(previousPath)
    ? (readJson(previousPath, 'graph') as QueryData)
    : null;
  const lastRefresh = readEvents(out, { rows: false }).findLast(
    e => e.type === 'refreshed'
  ) as Partial<RefreshSummary> | undefined;
  const kept = [
    out,
    ...registeredRuns(root).filter(r => resolve(r) !== resolve(out)),
  ]
    .map(dir => join(dir, SINCE_BASE))
    .filter(existsSync)
    .map(file => ({
      file,
      kept: readJson(file, 'kept base graph') as KeptBase,
    }))
    .find(k => k.kept.mergeBase === mergeBase);

  note(
    `refreshing ${out} (${plural(changed.length, 'file')} changed since ${rev})`
  );
  const refreshed = await refreshGraph(out);
  const after = readJson(dataPath, 'graph') as QueryData;
  const diff = new Set(changed);
  // A candidate is the merge base's graph when the graph files it differs in from now are the ones the diff lists.
  const matches = (D: QueryData, drift: string[]) => {
    const files = new Set([...graphFiles(after), ...graphFiles(D)]);
    const moved = new Set(drift.filter(f => files.has(f)));
    const listed = [...diff].filter(f => files.has(f));
    return moved.size === listed.length && listed.every(f => moved.has(f));
  };
  const candidates = [
    {
      label: 'the graph as it was before this refresh',
      data: before,
      drift: filesOf(refreshed),
    },
    ...(previous
      ? [
          {
            label: `the graph its previous refresh replaced (${previousPath})`,
            data: previous,
            drift: [...filesOf(lastRefresh), ...filesOf(refreshed)],
          },
        ]
      : []),
  ];
  const found = kept
    ? {
        label: `the base graph an earlier check kept (${kept.file})`,
        data: kept.kept.graph,
      }
    : candidates.find(c => matches(c.data, c.drift));
  if (found && !kept)
    writeJson(join(out, SINCE_BASE), {
      mergeBase,
      graph: found.data,
    } satisfies KeptBase);
  const short = mergeBase.slice(0, 12);
  const report = {
    rev,
    commit,
    mergeBase,
    changed,
    rebuilt: refreshed.chunks ?? [],
    base: found?.label ?? null,
    note: found
      ? `since ${rev} (merge base ${short}): ${plural(changed.length, 'changed file')}, rebuilt chunks: ${(refreshed.chunks ?? []).join(', ') || 'none'}; base: ${found.label}`
      : `no saved graph of ${rev} (merge base ${short}) to compare: the saved graphs differ from it in files the diff does not list, or miss files it lists; every violation is reported. Refresh or build the graph while ${rev} is checked out, then check --since ${rev} on the branch`,
  };
  return { after, base: found?.data ?? null, report };
}
