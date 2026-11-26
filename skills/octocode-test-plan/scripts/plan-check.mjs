#!/usr/bin/env node
// Mechanical checks of a saved test plan, and optionally its results file, against output.md.
import { readFileSync } from 'node:fs';
import { fail, errorMessage } from './lib/fail.mjs';
import { isMainModule } from './lib/entrypoint.mjs';

const HELP = `Usage: node <skill-folder>/scripts/plan-check.mjs <plan.md> [--results <results.md>]

Checks a saved test plan against the mechanical rules of output.md (Plan check): unique case IDs,
the required fields of every case, Features rows with a source mapped to defined cases or Not tested,
cases that no Features row maps, and a Discriminates field on every P0 case. With --results it also
checks the results table against the plan: one result per case from pass, fail, blocked or skipped,
skipped only for a case with a Skip if field, and evidence for every pass or fail.

Plan format (output.md and its references/plan-*.md): a Features table with Source and Cases columns; under Cases, each case is a
heading one level below it, "<ID> — <title>" (or "<ID>: <title>"), the ID one token with a digit
such as T1; other headings there are notes. Each field starts its own line as
"Name: text", "**Name:** text" or a "| Name | text |" row. Fields: Covers, Priority/level (naming P0,
P1 or P2), Given, When, Then, Pass if / Fail if, Evidence to capture, Cleanup; Discriminates on P0;
optional Skip if. A field in the Cases intro, before the first case, applies to every case.
Results format: a table with ID, Result and Evidence columns.

Output: one JSON object on stdout.
  ok                      true when every list below is empty
  plan, counts            the plan path; {features, cases, p0}
  structureErrors         sections, tables or columns the checker could not find
  duplicateIds            case IDs defined more than once
  missingFields           [{id, field}]: absent, blank or placeholder fields (Priority/level without P0-P2)
  featuresWithoutSource   Features rows with a blank Source
  featuresWithoutCases    Features rows that name no defined case and do not say Not tested
  unknownCaseRefs         [{feature, ref}]: Cases-column IDs no case defines
  orphanCases             cases no Features row names
  p0WithoutDiscriminates  P0 cases without a Discriminates field
  results                 with --results: {file, structureErrors, missingResults, unknownIds,
                          duplicateResults, invalidResults:[{id, result}], skippedNotPermitted,
                          withoutEvidence}
Effects: reads the files; writes nothing.
Exit codes: 0 no findings; 1 findings (ok:false); 2 invalid input, as {"ok":false,"error":{code,message}}
  with code usage, missing-file or unreadable.`;

const FIELDS = [
  ['Covers', 'covers'],
  ['Priority/level', 'priority(?:\\s*/\\s*level)?'],
  ['Given', 'given'],
  ['When', 'when'],
  ['Then', 'then'],
  ['Pass if / Fail if', 'pass if(?:\\s*/\\s*fail if)?|fail if|pass\\s*/\\s*fail'],
  ['Discriminates', 'discriminates'],
  ['Evidence to capture', 'evidence(?: to capture)?'],
  ['Cleanup', 'clean-?up'],
  ['Skip if', 'skip if'],
].map(([name, pattern]) => ({
  name,
  line: new RegExp(`^\\s*(?:[-*+]\\s+|\\d+[.)]\\s+)?(?:\\*\\*|__)?(?:${pattern})(?:\\*\\*|__)?\\s*:(?:\\*\\*|__)?\\s*(.*)$`, 'i'),
  row: new RegExp(`^\\s*\\|\\s*(?:\\*\\*|__)?(?:${pattern})(?:\\*\\*|__)?\\s*:?\\s*\\|(.*)$`, 'i'),
}));
const REQUIRED = ['Covers', 'Priority/level', 'Given', 'When', 'Then', 'Pass if / Fail if', 'Evidence to capture', 'Cleanup'];
const RESULTS = new Set(['pass', 'fail', 'blocked', 'skipped']);

/** Markdown inline formatting removed, whitespace collapsed. */
const clean = (text) => String(text).replace(/\*\*|__|`/g, '').replace(/\s+/g, ' ').trim();
const blank = (text) => /^(?:|[-–—]|tbd|todo|<[^>]*>)$/i.test(clean(text.replace(/\|/g, ' ')));
const unique = (items) => [...new Set(items)];

/** Lines without frontmatter, each marked when it sits inside a code fence. */
function scan(text) {
  const raw = String(text).replace(/\r\n?/g, '\n').split('\n');
  let start = 0;
  if (raw[0] === '---') {
    const end = raw.indexOf('---', 1);
    if (end > 0) start = end + 1;
  }
  const lines = [];
  let fence = null;
  for (const line of raw.slice(start)) {
    const marker = line.match(/^ {0,3}(`{3,}|~{3,})/);
    if (fence) {
      lines.push({ text: line, fenced: true });
      if (marker && marker[1][0] === fence[0] && marker[1].length >= fence.length) fence = null;
      continue;
    }
    if (marker) fence = marker[1];
    lines.push({ text: line, fenced: Boolean(marker) });
  }
  return lines;
}

function headingOf(line) {
  if (line.fenced) return null;
  const match = line.text.match(/^ {0,3}(#{1,6})\s+(.*?)\s*#*\s*$/);
  return match ? { level: match[1].length, title: clean(match[2]) } : null;
}

/** The lines under the first heading named `name`, up to the next heading of the same or a higher level. */
function section(lines, name) {
  for (let i = 0; i < lines.length; i++) {
    const heading = headingOf(lines[i]);
    if (!heading || heading.title.replace(/[:.]$/, '').toLowerCase() !== name.toLowerCase()) continue;
    let end = i + 1;
    while (end < lines.length && !(headingOf(lines[end])?.level <= heading.level)) end++;
    return { level: heading.level, lines: lines.slice(i + 1, end) };
  }
  return null;
}

const cells = (row) => row.trim().replace(/^\|/, '').replace(/\|$/, '').split(/(?<!\\)\|/).map((cell) => cell.trim());

/** Markdown tables in the lines, as {header, rows} with cell text. */
function tables(lines) {
  const found = [];
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const next = lines[i + 1];
    if (line.fenced || !line.text.trim().startsWith('|') || !next || next.fenced || !/^\s*\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)*\|?\s*$/.test(next.text)) continue;
    const table = { header: cells(line.text).map(clean), rows: [] };
    let j = i + 2;
    for (; j < lines.length && !lines[j].fenced && lines[j].text.trim().startsWith('|'); j++) table.rows.push(cells(lines[j].text));
    found.push(table);
    i = j - 1;
  }
  return found;
}

/** Fields in a range of lines: a label line starts a field, other lines continue the open one. */
function fieldsIn(lines) {
  const fields = new Map();
  let open = null;
  for (const line of lines) {
    if (headingOf(line)) { open = null; continue; }
    const field = line.fenced ? null : FIELDS.find((item) => item.line.test(line.text) || item.row.test(line.text));
    if (field) {
      const match = line.text.match(field.line) || line.text.match(field.row);
      open = field.name;
      fields.set(open, [...(fields.get(open) || []), match[1]]);
    } else if (open) {
      fields.get(open).push(line.text);
    }
  }
  // Only filled fields count: a blank or placeholder field is missing.
  return new Map([...fields].map(([name, parts]) => [name, parts.join('\n')]).filter(([, text]) => !blank(text)));
}

/** "<ID> — <title>" or "<ID>: <title>", where the ID is one token with a digit; anything else is a note. */
function caseId(title) {
  const match = title.match(/^(\S+)\s+[—–-]\s+\S/) || title.match(/^(\S+?):\s+\S/);
  return match && /\d/.test(match[1]) ? match[1] : null;
}

const idLike = (token) => /^[A-Za-z][\w.-]*\d[\w.-]*$/.test(token) && !/^P[0-2]$/.test(token);

/** "C1–C4", "C1-4" or "TC-01–TC-03" as the IDs it spans, or null. */
function expandRange(token) {
  const match = token.match(/^(.*?)(\d+)[–—-](.*?)(\d+)$/);
  if (!match) return null;
  const [, prefix, from, other, to] = match;
  if (!/^[A-Za-z]/.test(prefix) || (other && other !== prefix) || Number(from) > Number(to)) return null;
  const width = from.startsWith('0') ? from.length : 0;
  const ids = [];
  for (let n = Number(from); n <= Number(to); n++) ids.push(prefix + String(n).padStart(width, '0'));
  return ids;
}

/** Case IDs named in a cell: defined IDs, ranges of them, and ID-like tokens no case defines. */
function idsIn(cell, known) {
  const ids = [];
  const unknown = [];
  const text = clean(cell).replace(/\s*([–—])\s*/g, '$1');
  for (const token of text.split(/[\s,;/&]+/).map((item) => item.replace(/^[^\w]+|[^\w]+$/g, '')).filter(Boolean)) {
    if (known.has(token)) { ids.push(token); continue; }
    const range = expandRange(token);
    if (range) {
      for (const id of range) (known.has(id) ? ids : unknown).push(id);
    } else if (idLike(token)) {
      unknown.push(token);
    }
  }
  return { ids: unique(ids), unknown: unique(unknown) };
}

function readCases(lines, structureErrors) {
  const cases = [];
  const found = section(lines, 'Cases');
  if (!found) {
    structureErrors.push('No "Cases" section.');
    return { cases, shared: new Map() };
  }
  const level = found.level + 1;
  const starts = found.lines.map((line, index) => ({ heading: headingOf(line), index })).filter(({ heading }) => heading?.level === level);
  const firstChild = found.lines.findIndex((line) => headingOf(line));
  const shared = fieldsIn(found.lines.slice(0, firstChild === -1 ? found.lines.length : firstChild));
  starts.forEach(({ heading, index }, n) => {
    const id = caseId(heading.title);
    if (!id) return;
    const body = found.lines.slice(index + 1, n + 1 < starts.length ? starts[n + 1].index : found.lines.length);
    const fields = new Map([...shared, ...fieldsIn(body)]);
    const priority = clean(fields.get('Priority/level') || '').match(/\bP([0-2])\b/)?.[0] || null;
    cases.push({ id, fields, priority });
  });
  if (!cases.length) structureErrors.push(`No case headings ("${'#'.repeat(level)} <ID> — <title>") under Cases.`);
  return { cases, shared };
}

function readFeatures(lines, structureErrors) {
  const found = section(lines, 'Features');
  if (!found) {
    structureErrors.push('No "Features" section.');
    return [];
  }
  const table = tables(found.lines).find((item) => item.header.some((cell) => /^cases?$/i.test(cell)));
  if (!table) {
    structureErrors.push('Features has no table with a Cases column.');
    return [];
  }
  const casesAt = table.header.findIndex((cell) => /^cases?$/i.test(cell));
  const sourceAt = table.header.findIndex((cell) => /^source$/i.test(cell));
  if (sourceAt === -1) structureErrors.push('The Features table has no Source column.');
  return table.rows.map((row) => ({
    feature: clean(row[0] || ''),
    source: sourceAt === -1 ? null : row[sourceAt] || '',
    cases: row[casesAt] || '',
  }));
}

function checkResults(text, cases) {
  const known = new Set(cases.map((item) => item.id));
  const report = { structureErrors: [], missingResults: [], unknownIds: [], duplicateResults: [], invalidResults: [], skippedNotPermitted: [], withoutEvidence: [] };
  const found = tables(scan(text)).filter((table) => /^id\b/i.test(table.header[0] || '') && table.header.some((cell) => /^result$/i.test(cell)));
  if (!found.length) {
    report.structureErrors.push('No results table whose first column is ID and that has a Result column.');
    return report;
  }
  const seen = new Map();
  for (const table of found) {
    const resultAt = table.header.findIndex((cell) => /^result$/i.test(cell));
    const evidenceAt = table.header.findIndex((cell) => /evidence/i.test(cell));
    if (evidenceAt === -1) report.structureErrors.push('A results table has no Evidence column.');
    for (const row of table.rows) {
      const { ids, unknown } = idsIn(row[0] || '', known);
      report.unknownIds.push(...unknown);
      if (!ids.length && !unknown.length) report.unknownIds.push(clean(row[0] || ''));
      const result = clean(row[resultAt] || '').toLowerCase();
      const evidence = evidenceAt === -1 ? '' : row[evidenceAt] || '';
      for (const id of ids) {
        seen.set(id, (seen.get(id) || 0) + 1);
        if (!RESULTS.has(result)) report.invalidResults.push({ id, result: clean(row[resultAt] || '') });
        if (result === 'skipped' && !cases.find((item) => item.id === id).fields.has('Skip if')) report.skippedNotPermitted.push(id);
        if ((result === 'pass' || result === 'fail') && (blank(evidence) || /^(?:none|n\/?a)$/i.test(clean(evidence)))) report.withoutEvidence.push(id);
      }
    }
  }
  report.missingResults = [...known].filter((id) => !seen.has(id));
  report.duplicateResults = [...seen].filter(([, count]) => count > 1).map(([id]) => id);
  for (const key of ['unknownIds', 'skippedNotPermitted', 'withoutEvidence']) report[key] = unique(report[key]);
  return report;
}

/** Every finding for one plan text and, when given, its results text. */
export function checkPlan(planText, resultsText = null) {
  const lines = scan(planText);
  const structureErrors = [];
  const features = readFeatures(lines, structureErrors);
  const { cases } = readCases(lines, structureErrors);
  const counts = new Map();
  for (const item of cases) counts.set(item.id, (counts.get(item.id) || 0) + 1);
  const known = new Set(counts.keys());
  const missingFields = [];
  for (const item of cases) {
    for (const field of REQUIRED) {
      if (!item.fields.has(field) || (field === 'Priority/level' && !item.priority)) missingFields.push({ id: item.id, field });
    }
  }
  const mapped = new Set();
  const featuresWithoutCases = [];
  const unknownCaseRefs = [];
  for (const row of features) {
    if (/^not tested\b/i.test(clean(row.cases))) continue;
    const { ids, unknown } = idsIn(row.cases, known);
    ids.forEach((id) => mapped.add(id));
    unknown.forEach((ref) => unknownCaseRefs.push({ feature: row.feature, ref }));
    if (!ids.length) featuresWithoutCases.push(row.feature);
  }
  const report = {
    ok: false,
    counts: { features: features.length, cases: cases.length, p0: cases.filter((item) => item.priority === 'P0').length },
    structureErrors,
    duplicateIds: [...counts].filter(([, count]) => count > 1).map(([id]) => id),
    missingFields,
    featuresWithoutSource: features.filter((row) => row.source !== null && blank(row.source)).map((row) => row.feature),
    featuresWithoutCases,
    unknownCaseRefs,
    orphanCases: unique(cases.map((item) => item.id).filter((id) => !mapped.has(id))),
    p0WithoutDiscriminates: unique(cases.filter((item) => item.priority === 'P0' && !item.fields.has('Discriminates')).map((item) => item.id)),
  };
  if (resultsText !== null) report.results = checkResults(resultsText, cases);
  const lists = [report, report.results || {}].flatMap((part) => Object.values(part).filter(Array.isArray));
  report.ok = lists.every((list) => list.length === 0);
  return report;
}

function parseArgs(argv) {
  const args = { plan: '', results: null, help: false };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--help' || arg === '-h') args.help = true;
    else if (arg === '--results') {
      if (!argv[i + 1] || argv[i + 1].startsWith('--')) fail('usage', '--results needs a results file path.', 2);
      args.results = argv[++i];
    } else if (arg.startsWith('--results=')) args.results = arg.slice('--results='.length);
    else if (arg.startsWith('-')) fail('usage', `Unknown option ${arg}; see --help.`, 2);
    else if (args.plan) fail('usage', `One plan per run; got ${args.plan} and ${arg}.`, 2);
    else args.plan = arg;
  }
  if (!args.help && !args.plan) fail('usage', 'Give the plan path: plan-check.mjs <plan.md> [--results <results.md>].', 2);
  return args;
}

function read(path) {
  try {
    return readFileSync(path, 'utf8');
  } catch (error) {
    return fail(error?.code === 'ENOENT' ? 'missing-file' : 'unreadable', `${path}: ${errorMessage(error)}`, 2);
  }
}

if (isMainModule(import.meta.url)) {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    console.log(HELP);
    process.exit(0);
  }
  const report = checkPlan(read(args.plan), args.results === null ? null : read(args.results));
  if (report.results) report.results = { file: args.results, ...report.results };
  console.log(JSON.stringify({ ok: report.ok, plan: args.plan, ...report }, null, 2));
  process.exitCode = report.ok ? 0 : 1;
}
