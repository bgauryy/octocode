import { test } from 'node:test';
import assert from 'node:assert/strict';
import { compare, maxSatisfying, parseRange, satisfies } from '../lib/semver.mjs';

const cases = [
  ['7.0.2', '>=4.8.4 <6.1.0', false], ['6.0.3', '>=4.8.4 <6.1.0', true], ['6.1.0', '>=4.8.4 <6.1.0', false],
  ['26.13.1', '^20.19.0 || ^22.13.0 || >=24', true], ['21.0.0', '^20.19.0 || ^22.13.0 || >=24', false],
  ['8.3.8', '^6.4.0 || ^7.0.0 || ^8.0.0', true], ['9.0.0', '^6.4.0 || ^7.0.0 || ^8.0.0', false],
  ['0.2.9', '^0.2.3', true], ['0.3.0', '^0.2.3', false], ['0.0.4', '^0.0.3', false], ['0.0.3', '^0.0.3', true],
  ['1.2.9', '~1.2.3', true], ['1.3.0', '~1.2', false], ['1.9.0', '~1', true], ['2.0.0', '1.x', false], ['1.4.0', '1.x', true],
  ['2.0.0', '>1', true], ['1.9.9', '>1', false], ['1.2.0', '<=1.2', true], ['1.3.0', '<=1.2', false],
  ['2.3.9', '1.2.3 - 2.3', true], ['2.4.0', '1.2.3 - 2.3', false], ['3.0.0', '*', true], ['3.0.0', '', true],
  ['1.2.3', '>= 1.2.3', true], ['1.2.3', '=1.2.3', true], ['1.2.4', '1.2.3', false],
];

test('ranges match the way npm resolves them', () => {
  for (const [version, range, expected] of cases) assert.equal(satisfies(version, range), expected, `${version} in "${range}"`);
});

test('a prerelease matches only a range naming a prerelease of the same version', () => {
  assert.equal(satisfies('7.1.0-beta', '>=7'), false);
  assert.equal(satisfies('7.1.0-rc.1', '>=7.1.0-beta'), true);
  assert.equal(satisfies('2.0.0-alpha', '^1.0.0'), false);
});

test('ordering follows SemVer 2 precedence', () => {
  const sorted = ['1.0.0', '1.0.0-rc.1', '1.0.0-alpha.beta', '1.0.0-alpha.1', '1.0.0-alpha', '1.0.0-beta.11', '1.0.0-beta.2'].toSorted(compare);
  assert.deepEqual(sorted, ['1.0.0-alpha', '1.0.0-alpha.1', '1.0.0-alpha.beta', '1.0.0-beta.2', '1.0.0-beta.11', '1.0.0-rc.1', '1.0.0']);
});

test('maxSatisfying picks the highest matching release and null when none match', () => {
  assert.equal(maxSatisfying(['6.0.2', '6.0.3', '7.0.2', '7.1.0-beta'], '~6.0'), '6.0.3');
  assert.equal(maxSatisfying(['6.0.2', '7.0.2'], '>=8'), null);
});

test('invalid range syntax throws instead of matching', () => {
  assert.throws(() => parseRange('>=banana'));
});
