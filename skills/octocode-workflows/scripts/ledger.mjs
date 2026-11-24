#!/usr/bin/env node
// The workflow coverage ledger: validates row states, counts them, marks the rows a change makes stale, removes
// rediscovered duplicates, gates the "full verification" claim, and compares a before/after measurement.
// Built-ins only.
import { readFileSync, realpathSync, renameSync, writeFileSync } from 'node:fs';
import { isMainModule } from './lib/entrypoint.mjs';
import { errorMessage, fail } from './lib/fail.mjs';

const USAGE = `Usage:
  node ledger.mjs <coverage.md|coverage.jsonl> [--stale-after <name>]... [--dedupe] [--claim full] [--write]
  node ledger.mjs --before <run.json> --after <run.json>

Ledger input (the coverage ledger in references/verification.md):
  coverage.md     every Markdown table outside code fences whose header has the columns "Surface or scenario" and
                  "State"; other text and tables are ignored. Columns are found by heading, ignoring case:
                    Surface or scenario    the row's name: resolved identity plus configuration/scenario identity
                    Acceptance check       required; a verified row needs it filled
                    Deciding evidence and relevant conditions   required; a verified row needs it filled
                    State                  queued | verified | failed | unverified | stale (backticks ignored)
                    Depends on             optional; the rows or inputs (source, contract, configuration,
                                           fixture) the evidence relies on, separated by ";"
                  Other columns, such as "Gap or next action", are kept and not read.
  coverage.jsonl  one JSON object per line, keyed by the same headings, with string values; a missing key other
                  than "Surface or scenario" and "State" reads as an empty cell.
  Names are compared after removing backticks and collapsing whitespace; they are case-sensitive.

Ledger options:
  --stale-after <name>  <name> (a row's name or a Depends on entry) changed. That row and every row that depends on
                        it, directly or through other rows, changes from verified or failed to stale; queued,
                        unverified and stale rows keep their state. Repeatable. Run it before rechecking anything.
  --dedupe              group rows that share a name. A group keeps every row that is not queued (or its first row
                        when all are queued) and removes the other, queued rows: a rediscovered path carries no check.
                        A group that keeps two rows needs the orchestrator to merge their evidence.
  --claim full          exit 1 unless fullClaimAllowed.
  --write               apply staleMarked and the removed duplicates to the file (atomic replace; other text is
                        unchanged). Without it nothing is written and the output shows the ledger as if applied.

Ledger output: one JSON object:
  ok, mode "ledger", ledger (path), rows
  counts            rows per state, all five keys, after the marks and removals
  invalidStates     [{"line","row","state"}]: a State cell that is not one of the five states
  staleMarked       [{"line","row","from","via"}]: via is the chain from the --stale-after name to the row
  duplicates        with --dedupe: [{"row","lines","states","kept","removed"}] (line numbers)
  fullClaimAllowed  true when the ledger has rows and every row is verified with its Acceptance check and Deciding
                    evidence filled. It covers the ledger only: traversal exhaustion and source/live inventory
                    reconciliation are not checked here.
  blockingRows      [{"line","row","state","reason"}]: rows that prevent the claim; reason is the state,
                    invalid-state, no-acceptance-check or no-evidence
  written           true when --write changed the file

Compare input (--before/--after): the same acceptance check and fixed inputs replayed before and after one change:
  {"acceptancePassed": true|false,
   "metrics": {"<name with unit>": <number> | [<number>, ...]},   repeated samples for a metric that varies
   "evidence": ["<anchor>", ...]}                                  optional on both sides: what the result returned
Compare output: {"ok","mode":"compare","acceptancePassed":{"before","after"},"metrics","unpaired","lostEvidence"}
  metrics       [{"name","before","after","difference","relativePercent","verdict"}]; before/after are
                {"values","median","min","max","spread"}; difference is the after median minus the before median and
                relativePercent that difference over the before median, to 0.01 (null when that median is 0). verdict: equal;
                lower or higher (after against before) when |difference| exceeds the larger spread; else
                within-spread. A single sample per side has spread 0 and is compared exactly.
  unpaired      metric names present on one side only (not compared)
  lostEvidence  before evidence missing from after; null when neither side lists evidence

Effects: reads the files; only --write changes the ledger. Changes nothing else.
Exit 0: ledger with no invalid state, the --claim held, and no duplicate left or needing a merge under --dedupe;
  compare with the after run passing acceptance and no lost evidence.
1: any of those fails (compare: revert the change). 2: invalid input or options (unreadable or unparsable file, no
ledger table, a missing column, an empty row name, a --stale-after name that is neither a row nor a Depends on
entry): {"ok":false,"error":{"code","message"}}.`;

export const STATES = ['queued', 'verified', 'failed', 'unverified', 'stale'];
const COLUMNS = {
  name: 'surface or scenario',
  check: 'acceptance check',
  evidence: 'deciding evidence and relevant conditions',
  state: 'state',
  dependsOn: 'depends on',
};
const REQUIRED = ['name', 'check', 'evidence', 'state'];
const EVIDENCE_STATES = new Set(['verified', 'failed']);

export class InputError extends Error {
  constructor(code, message) {
    super(message);
    this.code = code;
  }
}

/** A row name or Depends on entry as compared: no backticks, single spaces. */
export const normalizeName = text => text.replaceAll('`', '').replace(/\s+/g, ' ').trim();
const normalizeState = text => text.replace(/[`*]/g, '').trim().toLowerCase();
const dependencies = text => text.split(';').map(normalizeName).filter(Boolean);

/** Offsets of the unescaped pipes in a line. */
const pipesOf = line => {
  const pipes = [];
  for (let i = 0; i < line.length; i++) {
    if (line[i] === '\\') i++;
    else if (line[i] === '|') pipes.push(i);
  }
  return pipes;
};

/** Cells of a Markdown table line, each with its trimmed text and its [start, end) span in the line. */
export function splitCells(line) {
  const pipes = pipesOf(line);
  const cells = [];
  let start = 0;
  for (const pipe of [...pipes, line.length]) {
    cells.push({ start, end: pipe, text: line.slice(start, pipe).trim().replaceAll('\\|', '|') });
    start = pipe + 1;
  }
  if (pipes.length && !line.slice(0, pipes[0]).trim()) cells.shift();
  if (pipes.length && cells.length > 1 && !line.slice(pipes.at(-1) + 1).trim()) cells.pop();
  return cells;
}

const hasPipe = line => pipesOf(line).length > 0;
const isDelimiter = cells => cells.length > 0 && cells.every(cell => /^:?-+:?$/.test(cell.text));

function columnsOf(headings, where, required = REQUIRED) {
  const index = {};
  for (const [key, heading] of Object.entries(COLUMNS)) {
    const at = headings.findIndex(text => text.trim().toLowerCase() === heading);
    if (at !== -1) index[key] = at;
  }
  const missing = required.filter(key => index[key] === undefined).map(key => `"${COLUMNS[key]}"`);
  if (missing.length) throw new InputError('missing-column', `${where}: the ledger needs the column ${missing.join(', ')}`);
  return index;
}

function rowOf(values, line, source) {
  const row = {
    line,
    name: normalizeName(values.name ?? ''),
    check: (values.check ?? '').trim(),
    evidence: (values.evidence ?? '').trim(),
    rawState: (values.state ?? '').trim(),
    dependsOn: dependencies(values.dependsOn ?? ''),
    source,
  };
  if (!row.name) throw new InputError('empty-row-name', `line ${line}: the "Surface or scenario" cell is empty`);
  row.state = normalizeState(row.rawState);
  return row;
}

/** Ledger rows from every coverage table in a Markdown file, and how many coverage tables it has. */
export function parseMarkdown(text) {
  const lines = text.split(/\r?\n/);
  const rows = [];
  let tables = 0;
  let fence = null;
  for (let i = 0; i < lines.length; i++) {
    const marker = /^\s{0,3}(`{3,}|~{3,})/.exec(lines[i]);
    if (marker) {
      if (!fence) fence = marker[1];
      else if (marker[1][0] === fence[0] && marker[1].length >= fence.length && /^\s*[`~]+\s*$/.test(lines[i])) fence = null;
      continue;
    }
    if (fence || !hasPipe(lines[i]) || i + 1 >= lines.length) continue;
    const header = splitCells(lines[i]);
    const delimiter = splitCells(lines[i + 1]);
    if (!isDelimiter(delimiter) || delimiter.length !== header.length) continue;
    const headings = header.map(cell => cell.text.toLowerCase());
    if (!headings.includes(COLUMNS.name) || !headings.includes(COLUMNS.state)) { i++; continue; }
    const index = columnsOf(headings, `line ${i + 1}`);
    tables++;
    i += 2;
    for (; i < lines.length && lines[i].trim() && hasPipe(lines[i]); i++) {
      const cells = splitCells(lines[i]);
      const values = Object.fromEntries(Object.entries(index).map(([key, at]) => [key, cells[at]?.text ?? '']));
      rows.push(rowOf(values, i + 1, { kind: 'markdown', stateCell: cells[index.state] ?? null }));
    }
    i--;
  }
  return { rows, tables };
}

/** Ledger rows from a JSONL file: one object per line keyed by the column headings. */
export function parseJsonl(text) {
  const rows = [];
  text.split(/\r?\n/).forEach((line, at) => {
    if (!line.trim()) return;
    let object;
    try { object = JSON.parse(line); } catch (error) { throw new InputError('invalid-json', `line ${at + 1}: ${errorMessage(error)}`); }
    if (!object || typeof object !== 'object' || Array.isArray(object)) throw new InputError('invalid-row', `line ${at + 1}: expected a JSON object`);
    const keys = Object.keys(object);
    const index = columnsOf(keys.map(key => key.toLowerCase()), `line ${at + 1}`, ['name', 'state']);
    const values = {};
    const stateKey = keys[index.state];
    for (const [field, position] of Object.entries(index)) {
      const value = object[keys[position]];
      if (value != null && typeof value !== 'string') throw new InputError('invalid-row', `line ${at + 1}: "${keys[position]}" must be a string`);
      values[field] = value ?? '';
    }
    rows.push(rowOf(values, at + 1, { kind: 'jsonl', object, stateKey }));
  });
  return rows;
}

/** Rows whose name or Depends on chain reaches a changed name, marked stale when they hold a check result. */
export function markStale(rows, names) {
  const byName = new Map();
  const dependents = new Map();
  for (const row of rows) {
    byName.set(row.name, [...(byName.get(row.name) ?? []), row]);
    for (const name of row.dependsOn) dependents.set(name, [...(dependents.get(name) ?? []), row]);
  }
  const marked = [];
  const reached = new Set();
  const queue = [];
  for (const raw of names) {
    const name = normalizeName(raw);
    if (!byName.has(name) && !dependents.has(name)) throw new InputError('unknown-name', `--stale-after ${JSON.stringify(raw)} is neither a row name nor a Depends on entry`);
    queue.push({ name, via: [name] });
  }
  const seen = new Set();
  while (queue.length) {
    const { name, via } = queue.shift();
    if (seen.has(name)) continue;
    seen.add(name);
    const affected = [...(byName.get(name) ?? []).map(row => ({ row, via })), ...(dependents.get(name) ?? []).map(row => ({ row, via: [...via, row.name] }))];
    for (const { row, via: path } of affected) {
      if (reached.has(row)) continue;
      reached.add(row);
      if (EVIDENCE_STATES.has(row.state)) {
        marked.push({ line: row.line, row: row.name, from: row.state, via: path });
        row.state = 'stale';
        row.marked = true;
      }
      queue.push({ name: row.name, via: path });
    }
  }
  return marked.toSorted((a, b) => a.line - b.line);
}

/** Groups of rows sharing a name; queued repeats beyond the kept rows are flagged removed. */
export function dedupe(rows) {
  const groups = new Map();
  for (const row of rows) groups.set(row.name, [...(groups.get(row.name) ?? []), row]);
  const duplicates = [];
  for (const [name, group] of groups) {
    if (group.length < 2) continue;
    const checked = group.filter(row => row.state !== 'queued');
    const kept = checked.length ? checked : [group[0]];
    for (const row of group) if (!kept.includes(row)) row.removed = true;
    duplicates.push({ row: name, lines: group.map(row => row.line), states: group.map(row => row.rawState), kept: kept.map(row => row.line), removed: group.filter(row => row.removed).map(row => row.line) });
  }
  return duplicates;
}

/** Counts, invalid states and the full-verification gate over the rows that remain. */
export function summarize(rows) {
  const remaining = rows.filter(row => !row.removed);
  const counts = Object.fromEntries(STATES.map(state => [state, 0]));
  const invalidStates = [];
  const blockingRows = [];
  for (const row of remaining) {
    const valid = STATES.includes(row.state);
    if (valid) counts[row.state]++;
    else invalidStates.push({ line: row.line, row: row.name, state: row.rawState });
    const reason = !valid ? 'invalid-state'
      : row.state !== 'verified' ? row.state
        : !row.check ? 'no-acceptance-check'
          : !row.evidence ? 'no-evidence'
            : null;
    if (reason) blockingRows.push({ line: row.line, row: row.name, state: valid ? row.state : row.rawState, reason });
  }
  return { rows: remaining.length, counts, invalidStates, fullClaimAllowed: remaining.length > 0 && blockingRows.length === 0, blockingRows };
}

/** The file text with marked rows set to stale and removed rows dropped; null when nothing changes. */
export function rewrite(text, rows) {
  const changed = rows.filter(row => row.marked || row.removed);
  if (!changed.length) return null;
  const eol = text.includes('\r\n') ? '\r\n' : '\n';
  const lines = text.split(/\r?\n/);
  for (const row of changed) {
    const at = row.line - 1;
    if (row.removed) { lines[at] = null; continue; }
    if (row.source.kind === 'jsonl') {
      lines[at] = JSON.stringify({ ...row.source.object, [row.source.stateKey]: 'stale' });
      continue;
    }
    const cell = row.source.stateCell;
    const raw = lines[at].slice(cell.start, cell.end);
    const word = new RegExp(row.rawState.replace(/[`*]/g, '').trim().replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'i');
    lines[at] = `${lines[at].slice(0, cell.start)}${raw.replace(word, 'stale')}${lines[at].slice(cell.end)}`;
  }
  return lines.filter(line => line !== null).join(eol);
}

/** The ledger analysis; with write, the file is replaced when a mark or removal applies. */
export function analyzeLedger(path, { staleAfter = [], dedupe: group = false, claim = null, write = false } = {}) {
  if (!/\.(md|jsonl)$/i.test(path)) throw new InputError('unsupported-ledger', `${path}: expected a .md or .jsonl ledger`);
  let text;
  try { text = readFileSync(path, 'utf8'); } catch (error) { throw new InputError('unreadable', `${path}: ${error.code ?? errorMessage(error)}`); }
  const jsonl = /\.jsonl$/i.test(path);
  const markdown = jsonl ? null : parseMarkdown(text);
  if (markdown?.tables === 0) throw new InputError('no-ledger-table', `${path}: no table with "Surface or scenario" and "State" columns`);
  const rows = markdown ? markdown.rows : parseJsonl(text);
  const staleMarked = staleAfter.length ? markStale(rows, staleAfter) : [];
  const duplicates = group ? dedupe(rows) : undefined;
  const summary = summarize(rows);
  let written = false;
  if (write) {
    const next = rewrite(text, rows);
    if (next !== null) {
      const target = realpathSync(path);
      const temporary = `${target}.${process.pid}.tmp`;
      writeFileSync(temporary, next);
      renameSync(temporary, target);
      written = true;
    }
  }
  const duplicateLeft = duplicates?.some(item => item.kept.length > 1 || (!write && item.removed.length > 0)) ?? false;
  const ok = summary.invalidStates.length === 0 && (claim !== 'full' || summary.fullClaimAllowed) && !duplicateLeft;
  const { rows: count, counts, invalidStates, fullClaimAllowed, blockingRows } = summary;
  return { ok, mode: 'ledger', ledger: path, rows: count, counts, invalidStates, staleMarked, ...(duplicates ? { duplicates } : {}), fullClaimAllowed, blockingRows, written };
}

const median = sorted => (sorted.length % 2 ? sorted[(sorted.length - 1) / 2] : (sorted[sorted.length / 2 - 1] + sorted[sorted.length / 2]) / 2);
const round = value => Number(value.toPrecision(12));

function samples(value, where) {
  const values = Array.isArray(value) ? value : [value];
  if (!values.length || !values.every(item => typeof item === 'number' && Number.isFinite(item))) throw new InputError('invalid-run', `${where}: expected a finite number or a non-empty array of them`);
  const sorted = values.toSorted((a, b) => a - b);
  return { values, median: median(sorted), min: sorted[0], max: sorted.at(-1), spread: round(sorted.at(-1) - sorted[0]) };
}

function loadRun(run, side) {
  if (!run || typeof run !== 'object' || Array.isArray(run)) throw new InputError('invalid-run', `${side}: expected a JSON object`);
  if (typeof run.acceptancePassed !== 'boolean') throw new InputError('invalid-run', `${side}.acceptancePassed: expected true or false`);
  if (!run.metrics || typeof run.metrics !== 'object' || Array.isArray(run.metrics)) throw new InputError('invalid-run', `${side}.metrics: expected an object of metric name to number`);
  if (run.evidence !== undefined && !(Array.isArray(run.evidence) && run.evidence.every(item => typeof item === 'string'))) throw new InputError('invalid-run', `${side}.evidence: expected an array of strings`);
  return run;
}

/** Per-metric before/after delta against run-to-run spread, plus correctness and evidence kept. */
export function compareRuns(beforeRun, afterRun) {
  const before = loadRun(beforeRun, 'before');
  const after = loadRun(afterRun, 'after');
  if ((before.evidence === undefined) !== (after.evidence === undefined)) throw new InputError('invalid-run', 'evidence: list it on both sides or neither');
  const names = [...new Set([...Object.keys(before.metrics), ...Object.keys(after.metrics)])];
  const metrics = [];
  const unpaired = [];
  for (const name of names) {
    if (!(name in before.metrics) || !(name in after.metrics)) { unpaired.push(name); continue; }
    const b = samples(before.metrics[name], `before.metrics.${name}`);
    const a = samples(after.metrics[name], `after.metrics.${name}`);
    const difference = round(a.median - b.median);
    const exceeds = Math.abs(difference) > Math.max(b.spread, a.spread);
    const verdict = difference === 0 ? 'equal' : exceeds ? (difference < 0 ? 'lower' : 'higher') : 'within-spread';
    metrics.push({ name, before: b, after: a, difference, relativePercent: b.median === 0 ? null : Math.round((difference / Math.abs(b.median)) * 10_000) / 100, verdict });
  }
  const lostEvidence = before.evidence === undefined ? null : [...new Set(before.evidence)].filter(item => !after.evidence.includes(item));
  return {
    ok: after.acceptancePassed && !(lostEvidence?.length),
    mode: 'compare',
    acceptancePassed: { before: before.acceptancePassed, after: after.acceptancePassed },
    metrics,
    unpaired,
    lostEvidence,
  };
}

function readJson(path) {
  let text;
  try { text = readFileSync(path, 'utf8'); } catch (error) { throw new InputError('unreadable', `${path}: ${error.code ?? errorMessage(error)}`); }
  try { return JSON.parse(text); } catch (error) { throw new InputError('invalid-json', `${path}: ${errorMessage(error)}`); }
}

function parseArgs(argv) {
  const options = { ledger: null, staleAfter: [], dedupe: false, claim: null, write: false, before: null, after: null };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--help' || arg === '-h') { console.log(USAGE); process.exit(0); }
    if (arg === '--dedupe') { options.dedupe = true; continue; }
    if (arg === '--write') { options.write = true; continue; }
    if (!arg.startsWith('--')) {
      if (options.ledger) fail('invalid-args', `one ledger file only; ${arg} is a second one`, 2);
      options.ledger = arg;
      continue;
    }
    const value = argv[++i];
    if (value === undefined || value.startsWith('--')) fail('invalid-args', `${arg} needs a value; see --help`, 2);
    if (arg === '--stale-after') options.staleAfter.push(value);
    else if (arg === '--claim') {
      if (value !== 'full') fail('invalid-args', `--claim ${value}: the only claim is full`, 2);
      options.claim = value;
    } else if (arg === '--before') options.before = value;
    else if (arg === '--after') options.after = value;
    else fail('invalid-args', `unknown argument ${arg}; see --help`, 2);
  }
  const compare = options.before !== null || options.after !== null;
  if (compare && (options.before === null || options.after === null)) fail('invalid-args', 'give both --before and --after', 2);
  if (compare && (options.ledger || options.staleAfter.length || options.dedupe || options.claim || options.write)) fail('invalid-args', '--before/--after compare two runs and take no ledger options', 2);
  if (!compare && !options.ledger) fail('invalid-args', 'give a ledger file, or --before and --after; see --help', 2);
  return options;
}

export function main(argv = process.argv.slice(2)) {
  const options = parseArgs(argv);
  let result;
  try {
    result = options.ledger
      ? analyzeLedger(options.ledger, options)
      : compareRuns(readJson(options.before), readJson(options.after));
  } catch (error) {
    if (error instanceof InputError) fail(error.code, error.message, 2);
    fail('ledger-failed', errorMessage(error), 1);
  }
  console.log(JSON.stringify(result, null, 2));
  process.exit(result.ok ? 0 : 1);
}

if (isMainModule(import.meta.url)) main();
