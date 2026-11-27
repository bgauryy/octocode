#!/usr/bin/env node
// Compares the baseline run of the affected checks with the run after a change and splits failures into
// pre-existing, introduced and fixed, plus checks that ran before and not after. Built-ins only.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { extname, isAbsolute, join, relative, resolve } from 'node:path';
import { errorMessage, fail } from './lib/fail.mjs';
import { isMainModule } from './lib/entrypoint.mjs';

const USAGE = `Usage: node check-diff.mjs <before> <after> [--format auto|junit|tap|json|list]
                           [--before-root <dir>] [--after-root <dir>]

Compares the baseline run of the affected checks (<before>) with the run after the change (<after>). Each side is a
report file or a directory of report files (read recursively and merged; in a directory, auto reads only .xml, .tap,
.json, .jsonl and .ndjson files). Formats, detected per file unless --format names one:
  junit  JUnit XML: a <testcase> with <failure> or <error> fails, with <skipped> skips (pytest --junitxml, nextest,
         surefire, jest-junit, node --test-reporter=junit, go-junit-report)
  tap    TAP 13/14 including node:test subtests: "not ok" fails, "# SKIP" skips, "not ok ... # TODO" does not fail;
         a parent result counts only when it fails with no failing child (a hook failure)
  json   Jest or Vitest --json, Mocha JSON, Playwright JSON, pytest-json-report, go test -json and
         cargo test --format json lines, or generic [{"name","status"}], {"tests":[...]} or {"<name>":"<status>"}
  list   plain text, one failing check name per line (blank lines and # comments ignored). A list names every
         failure of a complete run: a name it omits counts as not failing, so a list cannot show missing checks.
A check is keyed by its full name (suite > case; Jest, Mocha and Playwright prefix the test file, made relative to
--before-root / --after-root, default the current directory, so a baseline from another checkout lines up).
A name reported more than once keeps its worst result (fail, then skip, then pass).

Output (stdout, JSON): {"ok","before","after","counts","introduced","preExisting","fixed","missing"}
  introduced   fails after and did not fail before ("before": "pass", "skip", "absent" or "notFailing" for a list)
  preExisting  fails before and after: baseline failures, not caused by the change
  fixed        failed before; passes after (or is absent from an after list)
  missing      passed or failed before; skipped or absent after ("after": "skip" | "absent"): unverified, not a pass
Reads the reports only; changes nothing.
Exit 0: nothing introduced. 1: at least one introduced failure. 2: invalid input (unreadable path, unknown format or
status, a structured report with no checks): {"ok":false,"error":{"code","message"}}.`;

const DIRECTORY_EXTENSIONS = new Set(['.xml', '.tap', '.json', '.jsonl', '.ndjson']);
const FORMATS = new Set(['auto', 'junit', 'tap', 'json', 'list']);
const RANK = { pass: 0, skip: 1, fail: 2 };
const STATUS_WORDS = {
  pass: ['pass', 'passed', 'ok', 'success', 'succeeded', 'successful', 'expected', 'flaky', 'neutral'],
  fail: ['fail', 'failed', 'failure', 'error', 'errored', 'broken', 'unexpected', 'timedout', 'timed_out', 'timeout', 'interrupted', 'crashed', 'action_required', 'startup_failure'],
  skip: ['skip', 'skipped', 'skipping', 'pending', 'ignored', 'todo', 'disabled', 'excluded', 'cancelled', 'canceled', 'cancel', 'queued', 'in_progress', 'waiting', 'xfailed', 'deselected'],
};
const STATUS = new Map(Object.entries(STATUS_WORDS).flatMap(([status, words]) => words.map(word => [word, status])));

class InputError extends Error {
  constructor(code, message) {
    super(message);
    this.code = code;
  }
}

/** The pass/fail/skip meaning of a reported status word; unknown words are invalid input, never guessed. */
export function normalizeStatus(value, where) {
  const status = STATUS.get(String(value ?? '').trim().toLowerCase().replaceAll(/[\s-]+/g, '_'));
  if (!status) throw new InputError('unknown-status', `${where}: unknown status ${JSON.stringify(value)}`);
  return status;
}

const ENTITIES = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'" };
const decodeEntities = text => text.replaceAll(/&(#x[0-9a-f]+|#\d+|[a-z]+);/gi, (whole, entity) => {
  if (entity[0] !== '#') return ENTITIES[entity.toLowerCase()] ?? whole;
  const code = entity[1].toLowerCase() === 'x' ? Number.parseInt(entity.slice(2), 16) : Number(entity.slice(1));
  return Number.isFinite(code) ? String.fromCodePoint(code) : whole;
});

function attributes(text) {
  const out = {};
  for (const [, name, double, single] of text.matchAll(/([^\s=]+)\s*=\s*(?:"([^"]*)"|'([^']*)')/g)) out[name] = decodeEntities(double ?? single);
  return out;
}

const joinName = parts => parts.filter(Boolean).filter((part, index, all) => part !== all[index - 1]).join(' > ');

// Markup that holds no tags, matched in one left-to-right pass so whichever opens first ends where XML ends it:
// a comment may mention "<![CDATA[" and a CDATA section "<!--".
const NON_TAG = /<!\[CDATA\[[\s\S]*?\]\]>|<!--[\s\S]*?-->|<\?[\s\S]*?\?>/g;

export function parseJunit(text) {
  // Repeated until stable, so text that a removal joins never forms a new section.
  let clean = text;
  for (let previous; clean !== previous;) {
    previous = clean;
    clean = clean.replaceAll(NON_TAG, '');
  }
  clean = clean.replaceAll(/<!DOCTYPE[^>]*>/gi, '');
  const tag = /<(\/?)([A-Za-z_][\w:.-]*)((?:\s+[^\s=/>]+\s*=\s*(?:"[^"]*"|'[^']*'))*)\s*(\/?)>/g;
  const suites = [];
  const rows = [];
  let open = null;
  for (const [, closing, tagName, attrText, selfClosing] of clean.matchAll(tag)) {
    const name = tagName.toLowerCase();
    if (name === 'testsuite') {
      if (closing) suites.pop();
      else if (!selfClosing) suites.push(attributes(attrText).name ?? '');
    } else if (name === 'testcase') {
      if (closing) {
        if (open) rows.push(open);
        open = null;
        continue;
      }
      const attrs = attributes(attrText);
      const row = { name: joinName([...suites, attrs.classname, attrs.name]), status: 'pass' };
      if (selfClosing) rows.push(row);
      else open = row;
    } else if (open && !closing && (name === 'failure' || name === 'error')) {
      open.status = 'fail';
    } else if (open && !closing && name === 'skipped' && open.status !== 'fail') {
      open.status = 'skip';
    }
  }
  if (open) rows.push(open);
  return rows;
}

/** Splits a TAP result's text at its first unescaped "#" into description and directive. */
function tapParts(text) {
  for (let i = 0; i < text.length; i++) {
    if (text[i] === '\\') { i++; continue; }
    if (text[i] === '#') return { description: text.slice(0, i), directive: text.slice(i + 1).trim() };
  }
  return { description: text, directive: '' };
}

export function parseTap(text) {
  const rows = [];
  const headers = []; // "# Subtest: name" lines: { indent, name }
  const results = []; // results not yet claimed by a shallower parent: { indent, fail }
  let inYaml = false;
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (inYaml) {
      if (line === '...') inYaml = false;
      continue;
    }
    if (line === '---') { inYaml = true; continue; }
    const indent = raw.length - raw.trimStart().length;
    const header = /^# Subtest: (.*)$/.exec(line);
    if (header) { headers.push({ indent, name: header[1].trim() }); continue; }
    if (/^Bail out!/i.test(line)) { rows.push({ name: 'Bail out!', status: 'fail' }); continue; }
    const result = /^(not )?ok\b\s*(\d+)?\s*(.*)$/.exec(line);
    if (!result) continue;
    while (headers.length && headers.at(-1).indent > indent) headers.pop();
    if (headers.length && headers.at(-1).indent === indent) headers.pop();
    const { description, directive } = tapParts(result[3]);
    const skip = /^skip\b/i.test(directive);
    const todo = /^todo\b/i.test(directive);
    const failed = Boolean(result[1]) && !todo;
    const children = [];
    for (let i = results.length - 1; i >= 0; i--) if (results[i].indent > indent) children.push(...results.splice(i, 1));
    results.push({ indent, fail: failed });
    // A parent result restates its children; keep it only for a failure no child explains (a hook failure).
    if (children.length && !(failed && !children.some(child => child.fail))) continue;
    const own = description.replace(/^-\s*/, '').replaceAll(/\\(.)/g, '$1').trim() || `#${result[2] ?? rows.length + 1}`;
    rows.push({ name: joinName([...headers.filter(entry => entry.indent < indent).map(entry => entry.name), own]), status: failed ? 'fail' : skip || todo ? 'skip' : 'pass' });
  }
  return rows;
}

export function parseList(text) {
  return text.split(/\r?\n/).map(line => line.trim()).filter(line => line && !line.startsWith('#')).map(name => ({ name, status: 'fail' }));
}

const relativeTo = (root, file) => {
  if (!file) return '';
  const text = String(file);
  if (!isAbsolute(text)) return text.replaceAll('\\', '/');
  const rel = relative(root, text);
  return (rel && !rel.startsWith('..') && !isAbsolute(rel) ? rel : text).replaceAll('\\', '/');
};

function jestRows(report, root, where) {
  const rows = [];
  for (const file of report.testResults) {
    const name = relativeTo(root, file.name ?? file.testFilePath);
    const assertions = file.assertionResults ?? [];
    for (const test of assertions) {
      const title = test.fullName ?? joinName([...(test.ancestorTitles ?? []), test.title]);
      rows.push({ name: joinName([name, title]), status: normalizeStatus(test.status, where) });
    }
    if (String(file.status).toLowerCase() === 'failed' && !rows.some(row => row.status === 'fail' && row.name.startsWith(`${name} > `))) rows.push({ name, status: 'fail' });
  }
  return rows;
}

function mochaRows(report, root) {
  const key = test => joinName([relativeTo(root, test.file), test.fullTitle ?? test.title]);
  const failed = new Set((report.failures ?? []).map(key));
  const pending = new Set((report.pending ?? []).map(key));
  const rows = report.tests.map(test => {
    const name = key(test);
    const erred = test.err && Object.keys(test.err).length > 0;
    return { name, status: failed.has(name) || erred ? 'fail' : pending.has(name) ? 'skip' : 'pass' };
  });
  const seen = new Set(rows.map(row => row.name));
  for (const name of failed) if (!seen.has(name)) rows.push({ name, status: 'fail' }); // hook failures
  return rows;
}

function playwrightRows(report, root, where) {
  const rows = [];
  const walk = (suite, titles) => {
    const path = [...titles, suite.file && !titles.length ? relativeTo(root, suite.file) : suite.title];
    for (const spec of suite.specs ?? []) {
      for (const test of spec.tests ?? []) {
        const status = test.status === 'expected' && test.expectedStatus === 'skipped' ? 'skip' : normalizeStatus(test.status, where);
        rows.push({ name: joinName([test.projectName ? `[${test.projectName}]` : '', ...path, spec.title]), status });
      }
    }
    for (const child of suite.suites ?? []) walk(child, path);
  };
  for (const suite of report.suites) walk(suite, []);
  return rows;
}

/** go test -json and cargo/libtest JSON event lines; the last terminal event per test wins. */
function eventRows(events, where) {
  const latest = new Map();
  for (const event of events) {
    if (typeof event.Action === 'string' && event.Package) {
      if (!['pass', 'fail', 'skip'].includes(event.Action)) continue;
      latest.set(event.Test ? `${event.Package} > ${event.Test}` : event.Package, normalizeStatus(event.Action, where));
    } else if (event.type === 'test' && typeof event.name === 'string' && ['ok', 'failed', 'ignored'].includes(event.event)) {
      latest.set(event.name, normalizeStatus(event.event, where));
    }
  }
  const failing = [...latest].filter(([, status]) => status === 'fail').map(([name]) => name);
  // A go package or parent test fails because a subtest did; keep it only when nothing beneath it failed.
  return [...latest].filter(([name, status]) => status !== 'fail' || !failing.some(other => other.startsWith(`${name}/`) || other.startsWith(`${name} > `)))
    .map(([name, status]) => ({ name, status }));
}

const NAME_FIELDS = ['nodeid', 'fullName', 'name', 'title', 'id'];
const STATUS_FIELDS = ['status', 'outcome', 'result', 'state', 'conclusion', 'bucket'];

function genericRows(items, where) {
  return items.map((item, index) => {
    const nameField = NAME_FIELDS.find(field => typeof item?.[field] === 'string' || typeof item?.[field] === 'number');
    const statusField = STATUS_FIELDS.find(field => typeof item?.[field] === 'string' || typeof item?.[field] === 'boolean');
    if (!nameField || !statusField) throw new InputError('unknown-format', `${where}: item ${index} has no name (${NAME_FIELDS.join('/')}) or status (${STATUS_FIELDS.join('/')}) field`);
    const value = item[statusField];
    return { name: String(item[nameField]), status: typeof value === 'boolean' ? (value ? 'pass' : 'fail') : normalizeStatus(value, where) };
  });
}

function parseJsonLines(text) {
  const events = [];
  for (const line of text.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed.startsWith('{')) continue;
    try { events.push(JSON.parse(trimmed)); } catch { /* interleaved test output is not an event */ }
  }
  return events;
}

export function parseJson(text, root, where) {
  let report;
  try {
    report = JSON.parse(text);
  } catch {
    const events = parseJsonLines(text);
    if (!events.length) throw new InputError('invalid-json', `${where}: neither JSON nor JSON lines`);
    return eventRows(events, where);
  }
  if (Array.isArray(report)) return genericRows(report, where);
  if (!report || typeof report !== 'object') throw new InputError('unknown-format', `${where}: JSON report is not an object or array`);
  if (Array.isArray(report.testResults)) return jestRows(report, root, where);
  if (report.stats && Array.isArray(report.tests)) return mochaRows(report, root);
  if (Array.isArray(report.suites) && report.config) return playwrightRows(report, root, where);
  if (typeof report.Action === 'string' || report.type === 'test' || report.type === 'suite') return eventRows([report], where);
  for (const field of ['tests', 'checks', 'results']) if (Array.isArray(report[field])) return genericRows(report[field], where);
  const entries = Object.entries(report);
  if (entries.length && entries.every(([, value]) => typeof value === 'string')) return entries.map(([name, value]) => ({ name, status: normalizeStatus(value, where) }));
  throw new InputError('unknown-format', `${where}: unrecognized JSON report shape`);
}

export function detectFormat(text) {
  const start = text.trimStart();
  if (start.startsWith('<')) return 'junit';
  if (start.startsWith('{') || start.startsWith('[')) return 'json';
  if (/^TAP version \d+/m.test(start) || /^\s*(?:not )?ok\b/m.test(start)) return 'tap';
  return 'list';
}

function reportFiles(path, format) {
  let stats;
  try { stats = statSync(path); } catch (error) { throw new InputError('unreadable', `${path}: ${error.code ?? errorMessage(error)}`); }
  if (!stats.isDirectory()) return [path];
  const wanted = format === 'list' ? new Set(['.txt', '.list']) : format === 'auto' ? DIRECTORY_EXTENSIONS : new Set({ junit: ['.xml'], tap: ['.tap'], json: ['.json', '.jsonl', '.ndjson'] }[format]);
  const files = [];
  const walk = dir => {
    for (const entry of readdirSync(dir, { withFileTypes: true }).toSorted((a, b) => a.name.localeCompare(b.name))) {
      if (entry.name.startsWith('.')) continue;
      const full = join(dir, entry.name);
      if (entry.isDirectory()) walk(full);
      else if (entry.isFile() && wanted.has(extname(entry.name).toLowerCase())) files.push(full);
    }
  };
  walk(path);
  if (!files.length) throw new InputError('no-reports', `${path}: no report files (${[...wanted].join(', ')})`);
  return files;
}

/** One side of the comparison: name → worst status, the formats read and whether it is only failure lists. */
export function readSide(path, { format = 'auto', root = process.cwd() } = {}) {
  const checks = new Map();
  const formats = new Set();
  for (const file of reportFiles(path, format)) {
    const text = readFileSync(file, 'utf8');
    const kind = format === 'auto' ? detectFormat(text) : format;
    formats.add(kind);
    const rows = kind === 'junit' ? parseJunit(text) : kind === 'tap' ? parseTap(text) : kind === 'json' ? parseJson(text, root, file) : parseList(text);
    if (!rows.length && kind !== 'list') throw new InputError('no-checks', `${file}: no checks found in this ${kind} report`);
    for (const { name, status } of rows) if (!checks.has(name) || RANK[status] > RANK[checks.get(name)]) checks.set(name, status);
  }
  return { checks, formats: [...formats].toSorted(), listOnly: formats.size === 1 && formats.has('list') };
}

const sideSummary = (path, side) => {
  const statuses = [...side.checks.values()];
  return { path, formats: side.formats, checks: statuses.length, failing: statuses.filter(s => s === 'fail').length, skipped: statuses.filter(s => s === 'skip').length };
};

export function compare(before, after) {
  const introduced = [];
  const preExisting = [];
  const fixed = [];
  const missing = [];
  const names = [...new Set([...before.checks.keys(), ...after.checks.keys()])].toSorted((a, b) => a.localeCompare(b));
  for (const name of names) {
    const was = before.checks.get(name) ?? (before.listOnly ? 'notFailing' : 'absent');
    const now = after.checks.get(name) ?? (after.listOnly ? 'notFailing' : 'absent');
    if (now === 'fail') {
      if (was === 'fail') preExisting.push(name);
      else introduced.push({ name, before: was });
    } else if (was === 'fail' && (now === 'pass' || now === 'notFailing')) {
      fixed.push(name);
    } else if ((was === 'pass' || was === 'fail') && (now === 'skip' || now === 'absent')) {
      missing.push({ name, before: was, after: now });
    }
  }
  return { introduced, preExisting, fixed, missing };
}

function parseArgs(argv) {
  const options = { paths: [], format: 'auto', beforeRoot: process.cwd(), afterRoot: process.cwd() };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--help' || arg === '-h') { console.log(USAGE); process.exit(0); }
    if (!arg.startsWith('--')) { options.paths.push(arg); continue; }
    const value = argv[++i];
    if (value === undefined) fail('invalid-args', `${arg} needs a value`, 2);
    if (arg === '--format' && FORMATS.has(value)) options.format = value;
    else if (arg === '--before-root') options.beforeRoot = resolve(value);
    else if (arg === '--after-root') options.afterRoot = resolve(value);
    else fail('invalid-args', `unknown option ${arg} ${value}; see --help`, 2);
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
    before = readSide(beforePath, { format: options.format, root: options.beforeRoot });
    after = readSide(afterPath, { format: options.format, root: options.afterRoot });
  } catch (error) {
    if (error instanceof InputError) fail(error.code, error.message, 2);
    fail('read-failed', errorMessage(error), 2);
  }
  const result = compare(before, after);
  const counts = Object.fromEntries(Object.entries(result).map(([key, rows]) => [key, rows.length]));
  console.log(JSON.stringify({ ok: result.introduced.length === 0, before: sideSummary(beforePath, before), after: sideSummary(afterPath, after), counts, ...result }, null, 2));
  process.exit(result.introduced.length ? 1 : 0);
}

if (isMainModule(import.meta.url)) main();
