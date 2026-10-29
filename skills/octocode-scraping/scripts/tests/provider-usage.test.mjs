import { test } from 'node:test';
import assert from 'node:assert/strict';
import { join } from 'node:path';
import { writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { scripts, withDir } from './helpers.mjs';

test('usage lookup is bounded, validates responses, and never prints the key', () => withDir((dir) => {
  const preload = join(dir, 'mock-fetch.mjs');
  writeFileSync(preload, `
globalThis.fetch = async (url, init) => {
  if (!(init.signal instanceof AbortSignal)) throw new Error('missing request deadline');
  const key = process.env.SCRAPING_ANT;
  const encoded = encodeURIComponent(key);
  switch (process.env.USAGE_CASE) {
    case 'success': return new Response(JSON.stringify({plan_name:'Starter', credits_remaining:123}), {status:200});
    case 'http': return new Response(JSON.stringify({detail:'denied '+key+' '+encoded}), {status:403});
    case 'reject': throw new Error('request failed '+key+' '+url.href);
    case 'timeout': throw Object.assign(new Error('timed out '+key), {name:'TimeoutError'});
    case 'null': return new Response('null', {status:200});
    case 'scalar': return new Response('42', {status:200});
    case 'array': return new Response('[]', {status:200});
    case 'malformed': return new Response('{oops', {status:200});
    case 'echo': return new Response(JSON.stringify({plan_name:key, detail:'query '+encoded}), {status:200});
    case 'long': return new Response(JSON.stringify({detail:'evidence '.repeat(100)+key+' final detail'}), {status:200});
    default: throw new Error('unknown test case');
  }
};
`);
  const key = 'fake+key&secret';
  const run = (name, value = key) => spawnSync(process.execPath, ['--import', preload, join(scripts, 'provider-usage.mjs')], {
    encoding: 'utf8', env: { ...process.env, SCRAPING_ANT: value, USAGE_CASE: name }, cwd: dir,
  });
  const cases = [
    ['success', 0, true], ['http', 1, false], ['reject', 1, false], ['timeout', 1, false],
    ['null', 1, false], ['scalar', 1, false], ['array', 1, false], ['malformed', 1, false], ['echo', 0, true],
  ];
  for (const [name, status, ok] of cases) {
    const result = run(name);
    assert.equal(result.status, status, `${name}: ${result.stdout}${result.stderr}`);
    assert.equal(result.stderr, '', name);
    assert.ok(!result.stdout.includes(key), `${name}: raw key leaked`);
    assert.ok(!result.stdout.includes(encodeURIComponent(key)), `${name}: encoded key leaked`);
    assert.equal(JSON.parse(result.stdout).ok, ok, name);
  }
  assert.equal(JSON.parse(run('success').stdout).creditsRemaining, 123);
  assert.equal(JSON.parse(run('http').stdout).status, 403);
  assert.equal(JSON.parse(run('long').stdout).detail, 'evidence '.repeat(100)+'[REDACTED] final detail');
  for (const name of ['null', 'scalar', 'array', 'malformed']) {
    assert.equal(JSON.parse(run(name).stdout).error.code, 'invalidResponse', name);
  }
  for (const name of ['reject', 'timeout']) assert.equal(JSON.parse(run(name).stdout).error.code, 'providerUnavailable', name);
  const spaced = run('reject', 'fake key');
  assert.equal(spaced.status, 1);
  assert.ok(!spaced.stdout.includes('fake key'));
  assert.ok(!spaced.stdout.includes('fake+key'));
}));
