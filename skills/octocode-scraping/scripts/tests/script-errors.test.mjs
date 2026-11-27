import { test } from 'node:test';
import assert from 'node:assert/strict';
import { join } from 'node:path';
import { node, withDir } from './helpers.mjs';

test('flags without values and every error share one stdout shape', () => withDir((dir) => {
  /** @type {[string, string[], number, string][]} */
  const cases = [
    ['provider-usage.mjs', ['--provider'], 2, 'invalidArguments'],
    ['corpus-run.mjs', ['--session-dir', '--regex', 'x'], 2, 'invalidArguments'],
    ['corpus-run.mjs', ['--session-dir', join(dir, 'missing'), '--regex', 'x'], 1, 'sessionNotFound'],
    ['corpus-run.mjs', ['--session-dir', dir, '--regex', '('], 2, 'invalidArguments'],
    ['source-query.mjs', [], 2, 'invalidArguments'],
    ['har-ingest.mjs', ['--session-dir', join(dir, 'missing'), '--har', 'x.har'], 1, 'sessionNotFound'],
    ['fetch.mjs', ['--url', 'https://x.test/', '--session', '..'], 2, 'invalidArguments'],
  ];
  for (const [script, args, exit, code] of cases) {
    const res = node(script, args, { cwd: dir });
    assert.equal(res.status, exit, `${script} ${args.join(' ')}: ${res.stdout}${res.stderr}`);
    const body = JSON.parse(res.stdout);
    assert.equal(body.ok, false);
    assert.equal(body.error.code, code, `${script}: ${res.stdout}`);
    assert.equal(typeof body.error.message, 'string');
  }
}));

test('provider-usage --check reports the key state offline and never prints the key', () => {
  const env = { ...process.env };
  delete env.SCRAPING_ANT;
  const missing = node('provider-usage.mjs', ['--check'], { env });
  assert.equal(missing.status, 1);
  assert.equal(JSON.parse(missing.stdout).error.code, 'missingKey');
  const set = node('provider-usage.mjs', ['--check'], { env: { ...env, SCRAPING_ANT: 'sk-secret-value' } });
  assert.equal(set.status, 0);
  assert.equal(JSON.parse(set.stdout).key, 'set');
  assert.ok(!set.stdout.includes('sk-secret-value'));
});
