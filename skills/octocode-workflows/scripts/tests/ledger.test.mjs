import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { STATES, compareRuns, parseMarkdown, splitCells } from '../ledger.mjs';

const script = fileURLToPath(new URL('../ledger.mjs', import.meta.url));
const HEADER = '| Surface or scenario | Acceptance check | Deciding evidence and relevant conditions | State | Depends on | Gap or next action |\n|---|---|---|---|---|---|';

function workspace(t) {
  const dir = mkdtempSync(join(tmpdir(), 'workflows-ledger-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  return (name, text) => {
    const file = join(dir, name);
    writeFileSync(file, text);
    return file;
  };
}

function run(args) {
  const result = spawnSync(process.execPath, [script, ...args], { encoding: 'utf8', env: { PATH: process.env.PATH } });
  return { status: result.status, json: JSON.parse(result.stdout) };
}

const table = rows => `# Coverage\n\n${HEADER}\n${rows.map(cells => `| ${cells.join(' | ')} |`).join('\n')}\n`;

test('states are validated and counted; a code-fenced example table is not part of the ledger', t => {
  const file = workspace(t)('coverage.md', `${table([
    ['`api` · default config', 'contract test', 'run log @abc', '`verified`', '', ''],
    ['client · empty input', 'e2e', '', 'queued', '', 'run e2e'],
    ['docs', 'link check', 'ok', 'Verified-ish', '', ''],
  ])}\n\`\`\`\n| Surface or scenario | State |\n|---|---|\n| example | verified |\n\`\`\`\n`);
  const { status, json } = run([file]);
  assert.equal(status, 1, 'an invalid state fails the run');
  assert.deepEqual(Object.keys(json.counts), STATES);
  assert.equal(json.rows, 3);
  assert.equal(Object.values(json.counts).reduce((sum, count) => sum + count, 0) + json.invalidStates.length, json.rows);
  assert.deepEqual(json.invalidStates, [{ line: 7, row: 'docs', state: 'Verified-ish' }]);
  assert.equal(json.counts.verified, 1);
});

test('--stale-after marks the changed row and its transitive dependents that hold a check result; --write edits only their State cells', t => {
  const file = workspace(t)('coverage.md', table([
    ['schema.json', 'regen diff empty', 'contract @abc', 'verified', '', ''],
    ['adapter · default config', 'adapter test', 'log @abc', 'failed', 'schema.json', ''],
    ['client · empty input', 'e2e', '', 'queued', 'adapter · default config', ''],
    ['cli · json output', 'snapshot', 'log @abc', '**verified**', 'client · empty input', ''],
    ['server · denial', 'denial check', '', 'unverified', '`schema.json`', 'needs token'],
    ['docs', 'link check', 'ok', 'verified', '', ''],
  ]));
  const before = readFileSync(file, 'utf8');
  const dry = run([file, '--stale-after', 'schema.json']);
  assert.equal(dry.status, 0);
  assert.equal(readFileSync(file, 'utf8'), before, 'nothing is written without --write');
  assert.deepEqual(dry.json.staleMarked.map(row => [row.row, row.from]), [['schema.json', 'verified'], ['adapter · default config', 'failed'], ['cli · json output', 'verified']]);
  assert.deepEqual(dry.json.staleMarked.at(-1).via, ['schema.json', 'adapter · default config', 'client · empty input', 'cli · json output'], 'a queued row still carries the dependency chain');
  assert.equal(dry.json.counts.queued, 1, 'a queued row keeps its state');
  assert.equal(dry.json.counts.unverified, 1, 'an unverified row keeps its state');
  assert.equal(dry.json.counts.verified, 1, 'an independent row keeps its evidence');

  const written = run([file, '--stale-after', 'schema.json', '--write']);
  assert.equal(written.json.written, true);
  const [oldLines, newLines] = [before, readFileSync(file, 'utf8')].map(text => text.split('\n'));
  assert.equal(newLines.length, oldLines.length);
  const changed = oldLines.flatMap((line, at) => (line === newLines[at] ? [] : [at + 1]));
  assert.deepEqual(changed, written.json.staleMarked.map(row => row.line));
  for (const line of changed) assert.equal(newLines[line - 1].replace('stale', 'X'), oldLines[line - 1].replace(/verified|failed/, 'X'), 'only the state word changes');
  assert.equal(run([file]).json.counts.stale, written.json.staleMarked.length, 'the rewritten ledger reads back with the marks');
});

test('a --stale-after name that is neither a row nor a Depends on entry is invalid input', t => {
  const file = workspace(t)('coverage.md', table([['api', 'test', 'log', 'verified', 'config', '']]));
  assert.equal(run([file, '--stale-after', 'config']).status, 0, 'a Depends on entry is a valid name');
  const { status, json } = run([file, '--stale-after', 'confg']);
  assert.equal(status, 2);
  assert.equal(json.error.code, 'unknown-name');
});

test('--dedupe removes rediscovered queued rows and leaves rows with check results for a merge', t => {
  const file = workspace(t)('coverage.md', table([
    ['client · empty input', 'e2e', '', 'queued', '', ''],
    ['`client` ·  empty input', 'e2e', '', 'queued', '', ''],
    ['api · default', 'test', 'log', 'verified', '', ''],
    ['api · default', 'test', '', 'queued', '', ''],
    ['api · denial', 'test', 'log', 'verified', '', ''],
  ]));
  const dry = run([file, '--dedupe']);
  assert.equal(dry.status, 1, 'duplicates remain in the file until --write');
  assert.deepEqual(dry.json.duplicates.map(group => [group.row, group.kept, group.removed]), [['client · empty input', [5], [6]], ['api · default', [7], [8]]]);
  assert.equal(dry.json.rows, 3, 'counts describe the ledger after the removals');
  const written = run([file, '--dedupe', '--write']);
  assert.equal(written.status, 0);
  assert.equal(run([file, '--dedupe']).json.duplicates.length, 0);

  const conflict = workspace(t)('conflict.md', table([['api', 'test', 'log a', 'verified', '', ''], ['api', 'test', 'log b', 'failed', '', '']]));
  const merge = run([conflict, '--dedupe', '--write']);
  assert.equal(merge.status, 1, 'two rows with check results need the orchestrator to merge them');
  assert.deepEqual(merge.json.duplicates[0].kept, [5, 6]);
  assert.equal(merge.json.written, false);
});

test('--claim full holds only when every row is verified with its check and evidence', t => {
  const write = workspace(t);
  const full = write('full.md', table([['api', 'test', 'log', 'verified', '', ''], ['cli', 'snapshot', 'log', 'verified', 'api', '']]));
  assert.deepEqual([run([full, '--claim', 'full']).status, run([full, '--claim', 'full']).json.fullClaimAllowed], [0, true]);

  const gaps = write('gaps.md', table([
    ['api', 'test', '', 'verified', '', ''],
    ['cli', '', 'log', 'verified', '', ''],
    ...STATES.filter(state => state !== 'verified').map(state => [state, 'test', 'log', state, '', '']),
  ]));
  const { status, json } = run([gaps, '--claim', 'full']);
  assert.equal(status, 1);
  assert.equal(json.fullClaimAllowed, false);
  assert.deepEqual(json.blockingRows.map(row => row.reason), ['no-evidence', 'no-acceptance-check', ...STATES.filter(state => state !== 'verified')]);
  assert.equal(run([gaps]).status, 0, 'without --claim the same ledger is valid');

  const empty = write('empty.md', `${HEADER}\n`);
  const none = run([empty, '--claim', 'full']);
  assert.deepEqual([none.status, none.json.rows, none.json.fullClaimAllowed], [1, 0, false]);

  const stale = run([full, '--stale-after', 'api', '--claim', 'full']);
  assert.equal(stale.status, 1, 'the claim reads the ledger after the stale marks');
});

test('a JSONL ledger uses the column headings as keys and --write updates the State key', t => {
  const rows = [
    { 'Surface or scenario': 'api', 'Acceptance check': 'test', 'Deciding evidence and relevant conditions': 'log', State: 'verified', 'Gap or next action': '' },
    { 'Surface or scenario': 'docs', State: 'queued' },
    { 'Surface or scenario': 'cli', 'Acceptance check': 'test', 'Deciding evidence and relevant conditions': 'log', State: 'verified', 'Depends on': 'api; config' },
  ];
  const file = workspace(t)('coverage.jsonl', `${rows.map(row => JSON.stringify(row)).join('\n')}\n`);
  const { status, json } = run([file, '--stale-after', 'config', '--write']);
  assert.equal(status, 0);
  assert.deepEqual(json.staleMarked.map(row => row.row), ['cli']);
  const lines = readFileSync(file, 'utf8').trim().split('\n').map(line => JSON.parse(line));
  assert.deepEqual(lines, [rows[0], rows[1], { ...rows[2], State: 'stale' }], 'only the State value changes');
  assert.deepEqual(Object.keys(lines[2]), Object.keys(rows[2]), 'key order is kept');
  assert.equal(json.counts.queued, 1, 'a row may omit keys other than its name and state');
});

test('structural defects are invalid input', t => {
  const write = workspace(t);
  const cases = [
    [write('none.md', '# Notes\n\n| a | b |\n|---|---|\n| 1 | 2 |\n'), 'no-ledger-table'],
    [write('column.md', '| Surface or scenario | State |\n|---|---|\n| api | verified |\n'), 'missing-column'],
    [write('name.md', table([['', 'test', 'log', 'verified', '', '']])), 'empty-row-name'],
    [write('bad.jsonl', '{"Surface or scenario": "api", "State": 1}\n'), 'invalid-row'],
    [write('nostate.jsonl', '{"Surface or scenario": "api", "Acceptance check": "t"}\n'), 'missing-column'],
    [write('ledger.txt', ''), 'unsupported-ledger'],
  ];
  for (const [file, code] of cases) {
    const { status, json } = run([file]);
    assert.deepEqual([status, json.ok, json.error.code], [2, false, code], file);
  }
  assert.equal(run(['--claim', 'some', cases[0][0]]).status, 2);
});

test('table cells split on unescaped pipes and keep their spans', () => {
  const line = '| a \\| b | `c` |  d  |';
  const cells = splitCells(line);
  assert.deepEqual(cells.map(cell => cell.text), ['a | b', '`c`', 'd']);
  for (const cell of cells) assert.equal(line.slice(cell.start, cell.end).trim().replaceAll('\\|', '|'), cell.text);
  assert.deepEqual(splitCells('a | b').map(cell => cell.text), ['a', 'b']);
  assert.equal(parseMarkdown(`${HEADER}\n| api | t | e | verified | | |\n\nafter | the table\n`).rows.length, 1, 'a blank line ends the table');
});

test('compare: a metric changes only beyond the larger run-to-run spread; lost correctness or evidence fails', t => {
  const result = compareRuns(
    { acceptancePassed: true, metrics: { tokens: 1200, wallMs: [300, 310, 305], calls: 4, only: 1 }, evidence: ['a:1', 'b:2'] },
    { acceptancePassed: true, metrics: { tokens: 900, wallMs: [302, 309, 300], calls: 4 }, evidence: ['b:2', 'a:1', 'c:3'] },
  );
  assert.equal(result.ok, true);
  const byName = Object.fromEntries(result.metrics.map(metric => [metric.name, metric]));
  assert.deepEqual([byName.tokens.verdict, byName.tokens.difference, byName.tokens.relativePercent], ['lower', -300, -25]);
  assert.equal(byName.wallMs.verdict, 'within-spread');
  assert.ok(Math.abs(byName.wallMs.difference) <= Math.max(byName.wallMs.before.spread, byName.wallMs.after.spread));
  assert.equal(byName.calls.verdict, 'equal');
  assert.deepEqual(result.unpaired, ['only']);
  assert.deepEqual(result.lostEvidence, []);

  const write = workspace(t);
  const side = (name, content) => write(name, JSON.stringify(content));
  const lost = run(['--before', side('b.json', { acceptancePassed: true, metrics: { tokens: 10 }, evidence: ['a:1'] }), '--after', side('a.json', { acceptancePassed: true, metrics: { tokens: 5 }, evidence: [] })]);
  assert.deepEqual([lost.status, lost.json.lostEvidence], [1, ['a:1']]);
  const broken = run(['--before', side('b2.json', { acceptancePassed: true, metrics: { tokens: 10 } }), '--after', side('a2.json', { acceptancePassed: false, metrics: { tokens: 5 } })]);
  assert.deepEqual([broken.status, broken.json.lostEvidence], [1, null]);
  const oneSided = run(['--before', side('b3.json', { acceptancePassed: true, metrics: {}, evidence: [] }), '--after', side('a3.json', { acceptancePassed: true, metrics: {} })]);
  assert.equal(oneSided.status, 2);
  assert.equal(run(['--before', side('b4.json', { metrics: {} }), '--after', side('a4.json', { acceptancePassed: true, metrics: {} })]).status, 2);
  assert.equal(run(['--before', 'x.json']).status, 2, 'compare needs both sides');
});
