#!/usr/bin/env node
// Reject a saved review whose verdict its own findings or checks contradict.
import { readFileSync } from 'node:fs';
import { isMainModule } from './lib/entrypoint.mjs';

const HELP = `Usage: node scripts/check-verdict.mjs <report.md>

Reads a saved review (output.md format): the YAML \`status\`, the \`## Findings\` table and the
\`## Checks\` table. Rejects APPROVE when a finding row carries \`(blocking)\` (as in
\`issue (blocking):\`) or a check's Result is \`failed\`.

Reads only; writes nothing.
Output (stdout, one JSON line):
  {"ok":true,"status","blocking":["<finding id>"],"checks":{"passed","failed","skipped","unavailable"},"conflicts":[]}
  On a conflict: the same fields with "ok":false, "conflicts":["..."] and "error":{"code":"verdictConflict","message"}
Exit codes: 0 verdict consistent; 1 conflict; 2 invalid input (unreadable file, missing \`status\`,
  missing ## Findings or ## Checks, a Checks table without a Result column, or a Result other than
  passed, failed, skipped or unavailable).`;

const RESULTS = ['passed', 'failed', 'skipped', 'unavailable'];
const STATUSES = ['APPROVE', 'COMMENT', 'REQUEST_CHANGES'];
const BLOCKING = /\((?:[^()]*[\s,])?blocking(?:[\s,][^()]*)?\)/i;

class Invalid extends Error {
  code = 'invalidReport';
}

const plain = cell => cell.replaceAll(/[`*_]/g, '').trim();
const cells = row => row.trim().replace(/^\|/, '').replace(/(?<!\\)\|$/, '').split(/(?<!\\)\|/).map(cell => cell.trim());

function status(text) {
  const front = text.match(/^---\r?\n([\s\S]*?)\r?\n---\r?\n/);
  const line = front?.[1].split(/\r?\n/).find(row => /^status\s*:/.test(row));
  if (!line) throw new Invalid('the report has no YAML `status`');
  const value = line.replace(/^status\s*:/, '').trim().replace(/^(['"])(.*)\1$/, '$2');
  if (!STATUSES.includes(value)) throw new Invalid(`status "${value}" is not one of ${STATUSES.join(', ')}`);
  return value;
}

// Lines of each `## ` section, outside fenced code.
function sections(text) {
  const found = new Map();
  let current = null;
  let fence = null;
  for (const line of text.split(/\r?\n/)) {
    const marker = line.match(/^\s*(`{3,}|~{3,})/)?.[1];
    if (marker && (!fence || marker[0] === fence[0] && marker.length >= fence.length)) fence = fence ? null : marker;
    if (!fence && !marker && /^#{1,2}\s/.test(line)) {
      current = /^##\s/.test(line) ? line.replace(/^##\s+/, '').trim().toLowerCase() : null;
      if (current) found.set(current, []);
      continue;
    }
    if (current && !fence) found.get(current).push(line);
  }
  return found;
}

// Data rows of the first table in a section, as cell arrays, with its header.
function table(lines) {
  const rows = [];
  for (const line of lines) {
    if (/^\s*\|/.test(line)) rows.push(cells(line));
    else if (rows.length) break;
  }
  if (rows.length < 2 || !rows[1].every(cell => /^:?-+:?$/.test(cell))) return null;
  return { header: rows[0].map(cell => plain(cell).toLowerCase()), rows: rows.slice(2) };
}

export function checkVerdict(text) {
  const verdict = status(text);
  const parts = sections(text);
  for (const name of ['findings', 'checks']) if (!parts.has(name)) throw new Invalid(`the report has no ## ${name[0].toUpperCase()}${name.slice(1)} section`);
  const findings = table(parts.get('findings'));
  const blocking = (findings?.rows ?? []).filter(row => BLOCKING.test(row.join('|'))).map(row => plain(row[0]));
  const checks = table(parts.get('checks'));
  const column = checks?.header.indexOf('result') ?? -1;
  if (column < 0) throw new Invalid('## Checks needs a table with a Result column');
  const counts = Object.fromEntries(RESULTS.map(result => [result, 0]));
  const failed = [];
  for (const row of checks.rows) {
    const result = plain(row[column] ?? '').toLowerCase().match(/^[a-z]+/)?.[0];
    if (!RESULTS.includes(result)) throw new Invalid(`check "${plain(row[0])}" has Result "${row[column] ?? ''}"; use ${RESULTS.join(', ')}`);
    counts[result] += 1;
    if (result === 'failed') failed.push(plain(row[0]));
  }
  const conflicts = verdict === 'APPROVE'
    ? [...blocking.map(id => `APPROVE with blocking finding ${id}`), ...failed.map(name => `APPROVE with failed check ${name}`)]
    : [];
  return { ok: conflicts.length === 0, status: verdict, blocking, checks: counts, conflicts };
}

function main(argv) {
  if (argv.includes('--help')) {
    console.log(HELP);
    return;
  }
  const fail = (code, message, exitCode, extra = {}) => {
    console.log(JSON.stringify({ ok: false, ...extra, error: { code, message } }));
    process.exitCode = exitCode;
  };
  if (argv.length !== 1) return fail('invalidArguments', 'expected one report path; see --help', 2);
  let result;
  try {
    result = checkVerdict(readFileSync(argv[0], 'utf8'));
  } catch (error) {
    return fail(error instanceof Invalid ? error.code : 'unreadableReport', error instanceof Error ? error.message : String(error), 2);
  }
  if (!result.ok) return fail('verdictConflict', 'the verdict contradicts its findings or checks: change the verdict or the finding', 1, result);
  console.log(JSON.stringify(result));
}

if (isMainModule(import.meta.url)) main(process.argv.slice(2));
