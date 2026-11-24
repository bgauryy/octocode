#!/usr/bin/env node
// The graph language battery's mechanical part: per row of references/battery.md, build and query the graph
// through the octocode-graph CLI on a frozen binary, run the ripgrep oracle, and write bench.json, compare.json
// (with --before) and report.md. Judging each row stays with the reader (battery.md#judge).
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { closeSync, existsSync, mkdirSync, openSync, readFileSync, readSync, statSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { errorMessage, fail } from './lib/fail.mjs';
import { isMainModule } from './lib/entrypoint.mjs';
import { nonCodeMask } from './lib/lexical.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(here, '../../..');
const BATTERY_MD = path.resolve(here, '../references/battery.md');
const GRAPH_CLI = path.join(REPO, 'packages/octocode-graph/bin/octocode-graph.mjs');
const PAGE = { limit: 1000, responseChars: 50000 };

const HELP = `Usage: node battery.mjs --bin <frozen binary> --out <new dir> [--rows <id,id>] [--before <bench.json>] [--repos <dir>]

Runs the rows of references/battery.md (all, or --rows ids from its Row column) one at a time:
graphBuild refresh, stats details, find (every page), callers per definition (confidence low, view
files, every page), the ripgrep oracle (declaration sites, exclude globs and strings/comments
dropped), unresolved on missed files, impact/issues/cycles/unresolved, and a second graphBuild
that must reuse the snapshot. Calls go through
  NODE_ENV=development OCTOCODE_NATIVE_BIN=<bin> node packages/octocode-graph/bin/octocode-graph.mjs
with the clone as cwd. --repos defaults to packages/octocode-benchmark/local/repos.

Writes into --out (created; refused when it already holds a bench.json):
  bench.json   one record per row with every call's input (fields: battery.md#fields)
  compare.json with --before <bench.json>: before beside after per row, changes and a verdict
               (regression | changed | same) that marks what to read; it does not accept a change
  report.md    the row table, before -> after with --before
stdout: JSON {ok, out, files, rows[]}. Exit 0 every row ran; 1 a row or tool failed (see rows[].error);
2 invalid input (missing --bin/--out, unknown row, unreadable --before, --out holds a bench.json).`;

/** The fields every row record carries, as battery.md#fields names them. */
export const FIELDS = [
  'ingest.ms', 'ingest.callSites', 'ingest.linked', 'ingest.callLinkRate', 'ingest.unresolvedByReason',
  'callers.pagesComplete',
  'recall.recall', 'recall.precision', 'recall.missed', 'recall.extra', 'recall.missedSites', 'recall.dropped',
  'workflowOk', 'incremental.ms', 'incremental.reused',
];
export const WORKFLOW = ['impact', 'issues', 'cycles', 'unresolved'];

const cell = (text) => text.trim().replaceAll('`', '');
const list = (text) => cell(text).split(',').map((item) => item.trim()).filter(Boolean);
const COLUMNS = { Row: ['lang', cell], Clone: ['repo', cell], 'Ingest dir': ['dir', cell], Probe: ['probe', cell], 'Oracle extensions': ['ext', list], Exclude: ['exclude', list] };

/** The battery rows, from the table in battery.md whose header starts with "Row | Clone". */
export function parseRows(markdown) {
  const lines = markdown.split('\n');
  const start = lines.findIndex((line) => /^\|\s*Row\s*\|\s*Clone\s*\|/.test(line));
  if (start < 0) throw new Error('battery.md has no "| Row | Clone |" table');
  const split = (line) => line.trim().replace(/^\||\|$/g, '').split('|');
  const header = split(lines[start]).map((name) => name.trim());
  for (const name of Object.keys(COLUMNS)) if (!header.includes(name)) throw new Error(`battery.md row table lacks the ${name} column`);
  const rows = [];
  for (const line of lines.slice(start + 2)) {
    if (!line.trim().startsWith('|')) break;
    const cells = split(line);
    const row = {};
    header.forEach((name, index) => {
      const column = COLUMNS[name];
      if (column) row[column[0]] = column[1](cells[index] ?? '');
    });
    rows.push(row);
  }
  return rows;
}

/** A glob (`*`, `**`, `?`) as a regex over a /-separated relative path. */
export function globRegex(glob) {
  let source = '';
  for (let i = 0; i < glob.length; i += 1) {
    const c = glob[i];
    if (c === '*' && glob[i + 1] === '*') {
      source += glob[i + 2] === '/' ? '(?:.*/)?' : '.*';
      i += glob[i + 2] === '/' ? 2 : 1;
    } else if (c === '*') source += '[^/]*';
    else if (c === '?') source += '[^/]';
    else source += c.replace(/[.+^${}()|[\]\\]/g, '\\$&');
  }
  return new RegExp(`^${source}$`);
}

/** The called name a graph `targetName` ends with (`ValidationUtils.ArgumentNotNull` -> `ArgumentNotNull`). */
export const calledName = (targetName) => String(targetName ?? '').replace(/<.*$/, '').split(/::|->|[.:#]/).pop();

const round = (value, digits) => (value == null ? null : Number(value.toFixed(digits)));
const ratio = (part, whole) => (whole ? part / whole : null);
const sortedSet = (values) => [...new Set(values)].toSorted();

/** Files relative to the ingest dir: the frame both sides of the ratio use. */
const underDir = (dir, file) => {
  const clean = file.replace(/^\.\//, '');
  return dir === '.' ? clean : clean.startsWith(`${dir}/`) ? clean.slice(dir.length + 1) : clean;
};
const inClone = (dir, file) => (dir === '.' ? file : `${dir}/${file}`);

/**
 * Recall and precision of the graph's caller files against oracle sites. sites: [{file, line, offset, nonCode}] in
 * ingest-dir-relative paths; definitions carry clone-relative paths and selectionByteRange.
 */
export function scoreOracle({ dir, exclude, sites, definitions, graphFiles }) {
  const excluded = exclude.map(globRegex);
  const isExcluded = (file) => excluded.some((regex) => regex.test(file));
  const declarations = new Set(definitions.filter((d) => d.selectionByteRange).map((d) => `${underDir(dir, d.path)}\0${d.selectionByteRange.startByte}`));
  const dropped = { definition: 0, excluded: 0, nonCode: 0 };
  const droppedSites = [];
  const kept = [];
  for (const site of sites) {
    const why = isExcluded(site.file) ? 'excluded' : declarations.has(`${site.file}\0${site.offset}`) ? 'definition' : site.nonCode ? 'nonCode' : null;
    if (!why) { kept.push(site); continue; }
    dropped[why] += 1;
    droppedSites.push({ file: site.file, line: site.line, offset: site.offset, why });
  }
  const oracle = sortedSet(kept.map((site) => site.file));
  const graph = sortedSet(graphFiles.map((file) => underDir(dir, file)).filter((file) => !isExcluded(file)));
  const graphSet = new Set(graph);
  const oracleSet = new Set(oracle);
  const both = oracle.filter((file) => graphSet.has(file)).length;
  return {
    exclude,
    dropped,
    droppedSites,
    rgFiles: oracle.length,
    graphFiles: graph.length,
    both,
    recall: ratio(both, oracle.length),
    precision: ratio(both, graph.length),
    missed: oracle.filter((file) => !graphSet.has(file)),
    extra: graph.filter((file) => !oracleSet.has(file)),
    oracleSites: kept.length,
    rawHits: sites.length,
    kept,
  };
}

/** The graph's reason at each oracle site of a missed file, from `unresolved` rows (clone-relative paths). */
export function missedSiteReasons({ dir, probe, kept, missed, unresolvedRows }) {
  const missedSet = new Set(missed);
  const byLine = new Map();
  for (const row of unresolvedRows) {
    if (calledName(row.targetName) !== probe) continue;
    const key = `${underDir(dir, row.path)}\0${row.line}`;
    if (!byLine.has(key)) byLine.set(key, row);
  }
  return kept.filter((site) => missedSet.has(site.file)).map((site) => {
    const row = byLine.get(`${site.file}\0${site.line}`);
    return { file: site.file, line: site.line, reason: row?.reason ?? null, targetName: row?.targetName ?? null };
  });
}

const get = (record, dotted) => dotted.split('.').reduce((value, key) => (value == null ? value : value[key]), record);
const countBy = (items, key) => items.reduce((counts, item) => ({ ...counts, [item[key] ?? 'none']: (counts[item[key] ?? 'none'] ?? 0) + 1 }), {});

/** The comparable view of one bench row (this script's or an earlier receipt with the same field names). */
export function rowSummary(row) {
  const workflow = Object.fromEntries(WORKFLOW.map((name) => [name, { error: Boolean(get(row, `workflow.${name}.error`)), total: get(row, `workflow.${name}.total`) ?? null }]));
  return {
    error: row.error ?? null,
    recall: get(row, 'recall.recall') ?? null,
    precision: get(row, 'recall.precision') ?? null,
    oracleFiles: get(row, 'recall.rgFiles') ?? null,
    graphFiles: get(row, 'recall.graphFiles') ?? null,
    missed: get(row, 'recall.missed') ?? [],
    extra: get(row, 'recall.extra') ?? [],
    missedSiteReasons: countBy(get(row, 'recall.missedSites') ?? [], 'reason'),
    callSites: get(row, 'ingest.callSites') ?? null,
    linked: get(row, 'ingest.linked') ?? null,
    callLinkRate: get(row, 'ingest.callLinkRate') ?? null,
    unresolvedByReason: get(row, 'ingest.unresolvedByReason') ?? null,
    callersPagesComplete: get(row, 'callers.pagesComplete') ?? null,
    workflow,
    workflowOk: row.workflowOk ?? null,
    reused: get(row, 'incremental.reused') ?? null,
    buildMs: get(row, 'ingest.ms') ?? null,
    reuseMs: get(row, 'incremental.ms') ?? null,
  };
}

const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

/** Before beside after for one row: what changed, what regressed, and the verdict that says what to read. */
export function compareRow(beforeRow, afterRow) {
  const before = rowSummary(beforeRow);
  const after = rowSummary(afterRow);
  const regressions = [];
  const changes = [];
  if (after.error && !before.error) regressions.push(`row failed: ${after.error}`);
  if (before.precision != null && (after.precision == null || after.precision < before.precision)) regressions.push('precision dropped');
  if (before.recall != null && (after.recall == null || after.recall < before.recall)) regressions.push('recall dropped');
  for (const name of WORKFLOW) if (after.workflow[name].error && !before.workflow[name].error) regressions.push(`${name} fails`);
  if (before.reused === true && after.reused !== true) regressions.push('snapshot no longer reused');
  if (before.callersPagesComplete === true && after.callersPagesComplete !== true) regressions.push('callers pages incomplete');
  for (const field of ['recall', 'precision', 'missed', 'extra', 'missedSiteReasons', 'callLinkRate', 'unresolvedByReason']) {
    if (!same(before[field], after[field])) changes.push(field);
  }
  for (const name of WORKFLOW) if (!same(before.workflow[name].total, after.workflow[name].total)) changes.push(`${name}.total`);
  const minus = (a, b) => a.filter((item) => !b.includes(item));
  return {
    lang: afterRow.lang,
    before,
    after,
    newMissed: minus(after.missed, before.missed),
    resolvedMissed: minus(before.missed, after.missed),
    newExtra: minus(after.extra, before.extra),
    removedExtra: minus(before.extra, after.extra),
    regressions,
    changes,
    verdict: regressions.length ? 'regression' : changes.length ? 'changed' : 'same',
  };
}

export function compareBench(beforeRows, afterRows) {
  const before = new Map(beforeRows.map((row) => [row.lang, row]));
  const after = new Set(afterRows.map((row) => row.lang));
  return {
    rows: afterRows.filter((row) => before.has(row.lang)).map((row) => compareRow(before.get(row.lang), row)),
    onlyBefore: [...before.keys()].filter((lang) => !after.has(lang)),
    onlyAfter: afterRows.map((row) => row.lang).filter((lang) => !before.has(lang)),
  };
}

const fixed = (value, digits) => (value == null ? '–' : value.toFixed(digits));
const arrow = (before, after, show) => (before === undefined ? show(after) : show(before) === show(after) ? show(after) : `${show(before)} → **${show(after)}**`);

/** report.md: identity, the row table (before → after with a comparison) and what each changed row needs. */
export function renderReport({ rows, comparison, binary, date, before }) {
  const byLang = new Map((comparison?.rows ?? []).map((entry) => [entry.lang, entry]));
  const status = comparison ? (comparison.rows.some((entry) => entry.verdict === 'regression') ? 'Regression' : comparison.rows.some((entry) => entry.verdict === 'changed') ? 'Changed' : 'Passed') : rows.some((row) => row.error) ? 'Failed' : 'Measured';
  const out = [
    '---',
    `status: ${status}`,
    `question: Graph language battery on ${binary.sha256.slice(0, 12)}${before ? `, against ${before}` : ''}: did any row regress?`,
    '---',
    `# Graph battery, ${date}`,
    '',
    `Binary \`${binary.path}\`, sha256 \`${binary.sha256}\`. Rows: ${rows.map((row) => row.lang).join(', ')}.`,
    comparison ? `Verdicts mark what to read, not acceptance: ${['regression', 'changed', 'same'].map((verdict) => `${comparison.rows.filter((entry) => entry.verdict === verdict).length} ${verdict}`).join(', ')}.` : 'No --before: one run, no verdicts.',
    '',
    '| Row | Recall | Precision | Link rate | Missed | Extra | Workflow | Reuse | Build ms | Verdict |',
    '|---|---|---|---|---|---|---|---|---|---|',
  ];
  for (const row of rows) {
    const entry = byLang.get(row.lang);
    const now = rowSummary(row);
    const then = entry?.before;
    const pick = (field) => (then ? then[field] : undefined);
    if (row.error) {
      out.push(`| ${row.lang} | error: ${row.error.replaceAll('|', '\\|')} | | | | | | | | ${entry?.verdict ?? ''} |`);
      continue;
    }
    out.push(`| ${[
      row.lang,
      arrow(pick('recall'), now.recall, (value) => fixed(value, 2)),
      arrow(pick('precision'), now.precision, (value) => fixed(value, 2)),
      arrow(pick('callLinkRate'), now.callLinkRate, (value) => fixed(value, 3)),
      arrow(then ? then.missed.length : undefined, now.missed.length, String),
      arrow(then ? then.extra.length : undefined, now.extra.length, String),
      now.workflowOk ? 'ok' : `fails: ${WORKFLOW.filter((name) => now.workflow[name].error).join(', ')}`,
      now.reused ? 'yes' : 'no',
      arrow(pick('buildMs'), now.buildMs, (value) => (value == null ? '–' : String(value))),
      entry?.verdict ?? '',
    ].join(' | ')} |`);
  }
  const toRead = (comparison?.rows ?? []).filter((entry) => entry.verdict !== 'same');
  if (toRead.length) {
    out.push('', '## To judge', '');
    for (const entry of toRead) {
      const parts = [...entry.regressions.map((text) => `**${text}**`), entry.changes.length ? `changed: ${entry.changes.join(', ')}` : null];
      for (const [label, files] of [['new misses', entry.newMissed], ['resolved misses', entry.resolvedMissed], ['new extras', entry.newExtra], ['removed extras', entry.removedExtra]]) {
        if (files.length) parts.push(`${label}: ${files.map((file) => `\`${file}\``).join(', ')}`);
      }
      out.push(`- **${entry.lang}** (${entry.verdict}): ${parts.filter(Boolean).join('; ')}`);
    }
  }
  const unscanned = rows.flatMap((row) => (row.recall?.unscannedExtensions ?? []).map((ext) => `${row.lang}: ${ext}`));
  if (unscanned.length) out.push('', `Extensions with no strings/comments lexer (their hits all count): ${unscanned.join('; ')}.`);
  out.push('', 'Receipts beside this file: `bench.json` (every row, every call input)' + (comparison ? ', `compare.json` (before beside after, verdict per row).' : '.'));
  return `${out.join('\n')}\n`;
}

function sha256(file) {
  const hash = createHash('sha256');
  const fd = openSync(file, 'r');
  const chunk = Buffer.alloc(1 << 20);
  try {
    for (let read = readSync(fd, chunk); read > 0; read = readSync(fd, chunk)) hash.update(chunk.subarray(0, read));
  } finally {
    closeSync(fd);
  }
  return hash.digest('hex');
}

/** One graph CLI call, recorded with its input, exit, wall time and error. */
function graphCall(ctx, tag, tool, input) {
  const started = Date.now();
  const run = spawnSync(process.execPath, [GRAPH_CLI, tool, '--json', JSON.stringify(input)], {
    cwd: ctx.cwd,
    env: { ...process.env, NODE_ENV: 'development', OCTOCODE_NATIVE_BIN: ctx.bin },
    encoding: 'utf8',
    maxBuffer: 1 << 30,
  });
  const ms = Date.now() - started;
  let reply = null;
  let error = null;
  try {
    reply = JSON.parse(run.stdout);
  } catch {
    error = `exit ${run.status}: unparseable output: ${(run.stderr || run.stdout || String(run.error ?? '')).trim().slice(0, 300)}`;
  }
  if (reply && typeof reply.error === 'string') error = `${reply.errorCode ?? 'error'}: ${reply.error}`;
  const failed = reply?.results?.find((result) => result?.status === 'error');
  if (failed) error = `${failed.data?.errorCode ?? 'error'}: ${failed.data?.error ?? 'error row'}`;
  ctx.calls.push({ seq: ctx.calls.length + 1, tag, tool, input, exit: run.status, ms, error });
  return { reply, data: reply?.results?.[0]?.data ?? null, next: reply?.next?.input ?? null, exit: run.status, ms, error };
}

/** A query and every page after it: the rows, the page count, whether the chain ended, and errors. */
function pagedQuery(ctx, tag, input) {
  const rows = [];
  const errors = [];
  let pages = 0;
  let ms = 0;
  let total = null;
  let call = graphCall(ctx, tag, 'graphQuery', input);
  for (;;) {
    pages += 1;
    ms += call.ms;
    if (call.error) { errors.push(call.error); break; }
    rows.push(...(call.data?.rows ?? []));
    total ??= call.data?.pagination?.total ?? null;
    if (!call.next) break;
    call = graphCall(ctx, `${tag}-next`, 'graphQuery', call.next);
  }
  return { rows, pages, ms, total, errors, pagesComplete: errors.length === 0 };
}

function git(cwd, args) {
  const run = spawnSync('git', ['-C', cwd, ...args], { encoding: 'utf8', maxBuffer: 1 << 26 });
  return run.status === 0 ? run.stdout : null;
}

/** ripgrep sites of the probe's call pattern under the ingest dir, each with its byte offset and a strings/comments flag. */
function oracleSites(cloneDir, row) {
  const cwd = path.join(cloneDir, row.dir);
  const pattern = `\\b${row.probe.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*[(<\\[]`;
  const run = spawnSync('rg', ['--json', ...row.ext.flatMap((ext) => ['-g', `*.${ext}`]), '-e', pattern, '.'], { cwd, encoding: 'utf8', maxBuffer: 1 << 30, stdio: ['ignore', 'pipe', 'pipe'] });
  if (run.status !== 0 && run.status !== 1) throw new Error(`rg exit ${run.status}: ${run.stderr.trim().slice(0, 300)}`);
  const sites = [];
  const masks = new Map();
  const unscanned = new Set();
  for (const line of run.stdout.split('\n')) {
    if (!line) continue;
    const event = JSON.parse(line);
    if (event.type !== 'match') continue;
    const file = event.data.path.text.replace(/^\.\//, '');
    if (!masks.has(file)) {
      const ext = path.extname(file).slice(1);
      const mask = nonCodeMask(readFileSync(path.join(cwd, file)), ext);
      if (!mask) unscanned.add(ext);
      masks.set(file, mask);
    }
    const mask = masks.get(file);
    for (const submatch of event.data.submatches) {
      const offset = event.data.absolute_offset + submatch.start;
      sites.push({ file, line: event.data.line_number, offset, nonCode: Boolean(mask?.[offset]) });
    }
  }
  return { sites, exit: run.status, unscannedExtensions: [...unscanned].toSorted() };
}

/** Every step of one battery row. */
export function runRow(row, { bin, binary, repos }) {
  const cloneDir = path.join(repos, row.repo);
  const record = { lang: row.lang, repo: row.repo, dir: row.dir, probe: row.probe, ext: row.ext, exclude: row.exclude, binary };
  if (!existsSync(path.join(cloneDir, row.dir))) return { ...record, error: `clone dir missing: ${path.join(cloneDir, row.dir)}`, calls: [] };
  const head = git(cloneDir, ['rev-parse', 'HEAD']);
  record.clone = { head: head?.trim() ?? null, statusShort: (git(cloneDir, ['status', '--short']) ?? '').split('\n').filter(Boolean).slice(0, 20) };
  const ctx = { cwd: cloneDir, bin, calls: [] };
  const build = graphCall(ctx, 'build-refresh', 'graphBuild', { queries: [{ path: row.dir, refresh: true }] });
  if (build.error || !build.data?.graph) return { ...record, error: build.error ?? 'graphBuild returned no graph', calls: ctx.calls };
  const graph = build.data.graph;
  const base = { path: row.dir, graph };
  const stats = graphCall(ctx, 'stats', 'graphQuery', { queries: [{ ...base, operation: 'stats', view: 'details' }] });
  const linkage = stats.data?.linkage ?? build.data.linkage ?? {};
  record.ingest = {
    ms: build.ms, exit: build.exit, error: null, graph, root: build.data.root,
    files: build.data.counts?.nodesByKind?.file ?? null, symbols: build.data.counts?.nodesByKind?.symbol ?? null, edges: build.data.counts?.edges ?? null,
    edgesByKind: build.data.counts?.edgesByKind ?? null, diagnostics: build.data.diagnostics ?? null, linkage, warnings: build.data.warnings ?? [],
    callSites: linkage.sites ?? null, linked: linkage.linked ?? null, callLinkRate: round(ratio(linkage.linked ?? 0, linkage.sites ?? 0), 3),
    unresolvedByReason: stats.data?.calls?.unresolvedByReason ?? null, imports: stats.data?.imports ?? null,
  };
  record.stats = { ms: stats.ms, exit: stats.exit, error: stats.error, body: stats.data };
  const find = pagedQuery(ctx, 'find', { queries: [{ ...base, operation: 'find', target: row.probe, kind: 'symbol', limit: PAGE.limit, includeProvenance: true }], responseChars: PAGE.responseChars });
  const definitions = find.rows.filter((found) => found.symbolName === row.probe).map(({ id, kind, path: file, line, selectionByteRange, params, returns }) => ({ id, kind, path: file, line, selectionByteRange, params, returns }));
  record.find = { ms: find.ms, total: find.total ?? find.rows.length, pages: find.pages, pagesComplete: find.pagesComplete, errors: find.errors, definitions };
  const perDefinition = {};
  const callerFiles = [];
  let callerRows = 0;
  let callerEdges = 0;
  let callerMs = 0;
  const callerErrors = [...find.errors];
  for (const definition of definitions) {
    const callers = pagedQuery(ctx, 'callers', { queries: [{ ...base, operation: 'callers', target: definition.id, confidence: 'low', limit: PAGE.limit, view: 'files' }], responseChars: PAGE.responseChars });
    const files = callers.rows.map((caller) => caller.path).filter(Boolean);
    perDefinition[definition.id] = { files: new Set(files).size, pages: callers.pages, total: callers.total ?? callers.rows.length };
    callerFiles.push(...files);
    callerRows += callers.rows.length;
    callerEdges += callers.rows.reduce((sum, caller) => sum + (caller.edges ?? 0), 0);
    callerMs += callers.ms;
    callerErrors.push(...callers.errors);
  }
  const fileList = sortedSet(callerFiles);
  record.callers = { definitions: definitions.length, rows: callerRows, edges: callerEdges, files: fileList.length, ms: callerMs, pagesComplete: callerErrors.length === 0 && find.pagesComplete, errors: callerErrors, perDefinition, fileList };
  const oracle = oracleSites(cloneDir, row);
  const { kept, ...score } = scoreOracle({ dir: row.dir, exclude: row.exclude, sites: oracle.sites, definitions, graphFiles: fileList });
  let missedSites = [];
  if (score.missed.length) {
    const unresolved = pagedQuery(ctx, 'unresolved-missed', { queries: [{ ...base, operation: 'unresolved', target: score.missed.map((file) => inClone(row.dir, file)), edges: ['calls'], limit: PAGE.limit }], responseChars: PAGE.responseChars });
    missedSites = missedSiteReasons({ dir: row.dir, probe: row.probe, kept, missed: score.missed, unresolvedRows: unresolved.rows });
    if (unresolved.errors.length) score.missedSitesErrors = unresolved.errors;
  }
  record.recall = { oracleExit: oracle.exit, ...score, missedSites, unscannedExtensions: oracle.unscannedExtensions };
  const workflowInputs = {
    impact: definitions[0] ? { ...base, operation: 'impact', target: definitions[0].id, limit: 20 } : null,
    issues: { ...base, operation: 'issues', limit: 200 },
    cycles: { ...base, operation: 'cycles', limit: 5 },
    unresolved: { ...base, operation: 'unresolved', limit: 5 },
  };
  record.workflow = {};
  for (const name of WORKFLOW) {
    if (!workflowInputs[name]) { record.workflow[name] = { ms: 0, exit: null, error: `no definition of ${row.probe} to query`, total: null }; continue; }
    const answer = graphCall(ctx, name, 'graphQuery', { queries: [workflowInputs[name]] });
    record.workflow[name] = { ms: answer.ms, exit: answer.exit, error: answer.error, total: answer.data?.pagination?.total ?? answer.data?.rows?.length ?? null };
  }
  record.workflowOk = WORKFLOW.every((name) => !record.workflow[name].error);
  const reuse = graphCall(ctx, 'build-reuse', 'graphBuild', { queries: [{ path: row.dir }] });
  record.incremental = { ms: reuse.ms, exit: reuse.exit, error: reuse.error, graph: reuse.data?.graph ?? null, reused: reuse.data?.reused === true && reuse.data?.graph === graph, warnings: reuse.data?.warnings ?? [] };
  record.calls = ctx.calls;
  return record;
}

function parseArgs(argv) {
  const options = {};
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--help' || arg === '-h') { options.help = true; continue; }
    if (!['--bin', '--out', '--rows', '--before', '--repos'].includes(arg)) fail('invalidInput', `unknown argument ${arg}; see --help`, 2);
    const value = argv[index + 1];
    if (!value || value.startsWith('--')) fail('invalidInput', `${arg} requires a value`, 2);
    options[arg.slice(2)] = value;
    index += 1;
  }
  return options;
}

function main(argv) {
  const options = parseArgs(argv);
  if (options.help) { console.log(HELP); return; }
  if (!options.bin || !options.out) fail('invalidInput', '--bin and --out are required; see --help', 2);
  const bin = path.resolve(options.bin);
  if (!existsSync(bin) || !statSync(bin).isFile()) fail('invalidInput', `--bin ${bin} is not a file`, 2);
  const out = path.resolve(options.out);
  if (existsSync(path.join(out, 'bench.json'))) fail('outExists', `${path.join(out, 'bench.json')} exists; give each run a new --out`, 2);
  const all = parseRows(readFileSync(BATTERY_MD, 'utf8'));
  const wanted = options.rows ? options.rows.split(',').map((id) => id.trim()).filter(Boolean) : all.map((row) => row.lang);
  const unknown = wanted.filter((id) => !all.some((row) => row.lang === id));
  if (unknown.length) fail('unknownRow', `unknown rows ${unknown.join(', ')}; rows: ${all.map((row) => row.lang).join(', ')}`, 2);
  let beforeRows = null;
  if (options.before) {
    try {
      beforeRows = JSON.parse(readFileSync(options.before, 'utf8'));
    } catch (error) {
      fail('invalidBefore', `cannot read --before ${options.before}: ${errorMessage(error)}`, 2);
    }
    if (!Array.isArray(beforeRows) || beforeRows.some((row) => typeof row?.lang !== 'string')) fail('invalidBefore', `--before ${options.before} is not a bench.json (an array of rows with lang)`, 2);
  }
  if (spawnSync('rg', ['--version']).status !== 0) fail('rgMissing', 'ripgrep (rg) is required for the oracle', 1);
  const binary = { path: bin, sha256: sha256(bin) };
  const repos = path.resolve(options.repos ?? path.join(REPO, 'packages/octocode-benchmark/local/repos'));
  const rows = [];
  for (const id of wanted) {
    const row = all.find((candidate) => candidate.lang === id);
    let record;
    try {
      record = runRow(row, { bin, binary, repos });
    } catch (error) {
      record = { lang: row.lang, repo: row.repo, dir: row.dir, probe: row.probe, binary, error: errorMessage(error), calls: [] };
    }
    rows.push(record);
    process.stderr.write(`battery: ${row.lang} ${record.error ? `error: ${record.error}` : `recall ${fixed(record.recall.recall, 2)} precision ${fixed(record.recall.precision, 2)}`}\n`);
  }
  mkdirSync(out, { recursive: true });
  const files = { bench: path.join(out, 'bench.json'), report: path.join(out, 'report.md') };
  writeFileSync(files.bench, `${JSON.stringify(rows, null, 1)}\n`);
  let comparison = null;
  if (beforeRows) {
    comparison = compareBench(beforeRows, rows);
    files.compare = path.join(out, 'compare.json');
    writeFileSync(files.compare, `${JSON.stringify({ before: path.resolve(options.before), binary, ...comparison }, null, 1)}\n`);
  }
  writeFileSync(files.report, renderReport({ rows, comparison, binary, date: new Date().toISOString().slice(0, 10), before: options.before ? path.resolve(options.before) : null }));
  const verdicts = new Map((comparison?.rows ?? []).map((entry) => [entry.lang, entry.verdict]));
  const failed = rows.some((row) => row.error || !row.workflowOk || !row.callers?.pagesComplete);
  console.log(JSON.stringify({
    ok: !failed,
    out,
    files,
    rows: rows.map((row) => ({ lang: row.lang, error: row.error ?? null, recall: row.recall?.recall ?? null, precision: row.recall?.precision ?? null, workflowOk: row.workflowOk ?? null, reused: row.incremental?.reused ?? null, verdict: verdicts.get(row.lang) ?? null })),
    ...(comparison ? { onlyBefore: comparison.onlyBefore, onlyAfter: comparison.onlyAfter } : {}),
  }, null, 1));
  if (failed) process.exitCode = 1;
}

if (isMainModule(import.meta.url)) {
  try {
    main(process.argv.slice(2));
  } catch (error) {
    fail('batteryFailed', errorMessage(error), 1);
  }
}
