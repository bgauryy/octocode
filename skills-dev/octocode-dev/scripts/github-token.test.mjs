import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { githubToken, tokenNames } from './lib/github-token.mjs';
import { ROOT } from './lib/workspaces.mjs';

const environment = JSON.parse(readFileSync(path.join(ROOT, 'packages/octocode-config/config-contract.json'), 'utf8')).environment;

test('script tokens follow the config contract token priority', () => {
  const names = tokenNames();
  assert.ok(names.length >= 2, 'the contract declares more than one token variable');
  for (const [index, name] of names.entries()) {
    assert.equal(typeof environment[name].tokenPriority, 'number');
    if (index) assert.ok(environment[names[index - 1]].tokenPriority < environment[name].tokenPriority);
  }
  const env = Object.fromEntries(names.map(name => [name, `value-of-${name}`]));
  assert.equal(githubToken(env), `value-of-${names[0]}`);
  assert.equal(githubToken({ ...env, [names[0]]: '  ' }), `value-of-${names[1]}`, 'a blank variable does not shadow the next one');
});
