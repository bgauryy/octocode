import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
const scripts = join(import.meta.dirname, '../../dist/engine');
function fixture(fn) {
  const cwd = mkdtempSync(join(tmpdir(), 'octo-evidence-audit-')),
    file = join(cwd, 'rows.json');
  writeFileSync(
    file,
    JSON.stringify(
      Array.from({ length: 15 }, (_, id) => ({ id, value: 'row-' + id }))
    )
  );
  const cli = args =>
    spawnSync(process.execPath, [join(scripts, 'cli.mjs'), ...args], {
      cwd,
      encoding: 'utf8',
      timeout: 10000,
    });
  try {
    fn({ cli, cwd, file });
  } finally {
    rmSync(cwd, { recursive: true, force: true });
  }
}
test('reading helpers reject unknown/duplicate flags and malformed predicates', () =>
  fixture(({ cli, file }) => {
    for (const args of [
      ['query', '--file', file, '--limt', '1'],
      ['artifact', '--file', file, '--formt', 'json'],
      ['query', '--file', file, '--limit', '1', '--limit', '2'],
      [
        'query',
        '--file',
        file,
        '--where',
        '[{"path":"/id","op":"eq","value":1,"typo":true}]',
      ],
      [
        'query',
        '--file',
        file,
        '--where',
        '[{"path":"/id","op":"eq","value":{}}]',
      ],
    ]) {
      const r = cli(args);
      assert.notEqual(r.status, 0, JSON.stringify(args));
      assert.equal(r.stdout, '');
    }
  }));
test('indexed paging verifies selected rows and rejects index corruption', () =>
  fixture(({ cli, file }) => {
    const result = cli(['query', '--file', file, '--limit', '3']);
    assert.equal(result.status, 0, result.stderr);
    const page = JSON.parse(result.stdout),
      index = JSON.parse(readFileSync(page.index, 'utf8')),
      rows = join(dirname(page.index), 'rows.jsonl'),
      offsets = join(dirname(page.index), 'offsets.bin');
    const original = readFileSync(rows);
    writeFileSync(
      rows,
      Buffer.from(original.toString().replace('row-0', 'bad-0'))
    );
    const corrupt = cli(['query', '--file', file, '--limit', '3']);
    assert.notEqual(corrupt.status, 0);
    assert.match(corrupt.stderr, /index.*changed/);
    writeFileSync(rows, original);
    const again = cli(['query', '--file', file, '--limit', '3']);
    assert.equal(again.status, 0, again.stderr);
    assert.equal(JSON.parse(again.stdout).indexReused, true);
    const bytes = readFileSync(offsets);
    bytes[0] ^= 1;
    writeFileSync(offsets, bytes);
    assert.notEqual(cli(['query', '--file', file]).status, 0);
    assert.equal(index.matched, 15);
  }));

test('numeric predicates compare source decimals exactly without rounding or overflow', () =>
  fixture(({ cli, file }) => {
    writeFileSync(
      file,
      '[{"id":"above","value":1.0000000000000001},{"id":"equal","value":1},{"id":"below","value":0.99999999999999999},{"id":"huge","value":1e10000},{"id":"tiny","value":1e-10000}]'
    );
    const check = (op, expected, ids) => {
      const r = cli([
        'query',
        '--file',
        file,
        '--where',
        JSON.stringify([{ path: '/value', op, value: expected }]),
      ]);
      assert.equal(r.status, 0, r.stderr);
      assert.deepEqual(
        JSON.parse(r.stdout).rows.map(row => row.value.id),
        ids
      );
    };
    check('eq', 1, ['equal']);
    check('gte', 1, ['above', 'equal', 'huge']);
    check('lte', 1, ['equal', 'below', 'tiny']);
    check('eq', 0, []);
  }));

test('raw numeric predicate lexemes remain exact across matching and continuation', () =>
  fixture(({ cli, file }) => {
    writeFileSync(
      file,
      '[{"id":"below","value":9007199254740992},{"id":"equal","value":9007199254740993},{"id":"above","value":9007199254740994}]'
    );
    const r = cli([
      'query',
      '--file',
      file,
      '--where',
      '[{"path":"/value","op":"gte","value":9007199254740993}]',
      '--limit',
      '1',
    ]);
    assert.equal(r.status, 0, r.stderr);
    const page = JSON.parse(r.stdout);
    assert.equal(page.matched, 2);
    assert.equal(page.rows[0].value.id, 'equal');
    const next = spawnSync(
      page.next.continue.command,
      page.next.continue.args,
      { encoding: 'utf8', timeout: 10000 }
    );
    assert.equal(next.status, 0, next.stderr);
    assert.deepEqual(
      JSON.parse(next.stdout).rows.map(row => row.value.id),
      ['above']
    );
  }));
