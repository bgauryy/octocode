import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  mkdtempSync,
  writeFileSync,
  rmSync,
  existsSync,
  mkdirSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import test from 'node:test';

const engine = resolve(import.meta.dirname, '../../dist/engine');
const cases = [
  ['cdp-checks/snapshot-query.mjs', '--limit', ['--file', 'snapshot.json']],
  [
    'cdp-checks/api-replay.mjs',
    '--max-chars',
    ['--response-file', 'response.json'],
  ],
  ['cdp-checks/har-pager.mjs', '--page-size', ['input.har']],
  ['cdp-checks/measure-query.mjs', '--limit', ['--perf', 'perf.json']],
  ['prune-artifacts.mjs', '--max-count', ['--dry-run']],
  ['protocol-corpus.mjs', '--domains', []],
  ['cdp-checks/har-redact.mjs', '--out', ['input.har']],
  ['cookie-bridge.mjs', '--to-port', []],
];
for (const [script, valueFlag, valid] of cases) {
  test(`${script} rejects malformed flags before reading or writing evidence`, () => {
    const cwd = mkdtempSync(join(tmpdir(), 'chrome-helper-inputs-'));
    try {
      writeFileSync(
        join(cwd, 'snapshot.json'),
        JSON.stringify({ refs: { a: {} }, outline: [] })
      );
      writeFileSync(
        join(cwd, 'response.json'),
        JSON.stringify({ response: {}, text: 'saved' })
      );
      writeFileSync(
        join(cwd, 'input.har'),
        JSON.stringify({ log: { entries: [] } })
      );
      writeFileSync(join(cwd, 'perf.json'), '{}');
      mkdirSync(join(cwd, 'fake-scraping', 'scripts'), { recursive: true });
      writeFileSync(
        join(cwd, 'fake-scraping', 'scripts', 'fetch.mjs'),
        'console.log(JSON.stringify({ok:false})); process.exitCode=1;'
      );
      const base =
        script === 'protocol-corpus.mjs'
          ? [...valid, '--scraping-skill-dir', join(cwd, 'fake-scraping')]
          : valid;
      for (const [args, expected] of [
        [
          [...base, '--unexpected-helper-flag'],
          /Unknown option --unexpected-helper-flag/,
        ],
        [[...base, valueFlag, 'one', valueFlag, 'two'], /Duplicate option/],
        [[...base, valueFlag], /needs a value/],
      ]) {
        const result = spawnSync(
          process.execPath,
          [join(engine, script), ...args],
          { cwd, encoding: 'utf8', timeout: 3000 }
        );
        assert.notEqual(result.status, 0, JSON.stringify(result));
        assert.match(result.stdout + result.stderr, expected);
        assert.equal(
          existsSync(join(cwd, '.octocode')),
          false,
          'invalid flags created output artifacts'
        );
      }
    } finally {
      rmSync(cwd, { recursive: true, force: true });
    }
  });
}
test('HAR readers retain their positional input and supported flags', () => {
  const cwd = mkdtempSync(join(tmpdir(), 'chrome-helper-valid-'));
  try {
    writeFileSync(
      join(cwd, 'input.har'),
      JSON.stringify({ log: { entries: [] } })
    );
    for (const args of [
      ['input.har', '--page', '1', '--format', 'json'],
      ['--page', '1', 'input.har', '--format', 'json'],
    ]) {
      const result = spawnSync(
        process.execPath,
        [join(engine, 'cdp-checks/har-pager.mjs'), ...args],
        { cwd, encoding: 'utf8' }
      );
      assert.equal(result.status, 0, result.stderr);
      assert.equal(JSON.parse(result.stdout).totalRows, 0);
    }
    const redact = spawnSync(
      process.execPath,
      [
        join(engine, 'cdp-checks/har-redact.mjs'),
        'input.har',
        '--out',
        '.octocode/redacted.har',
        '--strip-bodies',
      ],
      { cwd, encoding: 'utf8' }
    );
    assert.equal(redact.status, 0, redact.stderr);
    assert.equal(existsSync(join(cwd, '.octocode/redacted.har')), true);
  } finally {
    rmSync(cwd, { recursive: true, force: true });
  }
});

test('measure reader rejects requested missing and malformed artifacts', () => {
  const cwd = mkdtempSync(join(tmpdir(), 'chrome-measure-invalid-'));
  try {
    writeFileSync(join(cwd, 'malformed.json'), '{broken');
    for (const file of ['missing.json', 'malformed.json']) {
      const result = spawnSync(
        process.execPath,
        [join(engine, 'cdp-checks/measure-query.mjs'), '--perf', file],
        { cwd, encoding: 'utf8' }
      );
      assert.notEqual(result.status, 0, result.stdout);
      assert.match(result.stderr, /Cannot read measurement artifact/);
    }
  } finally {
    rmSync(cwd, { recursive: true, force: true });
  }
});
