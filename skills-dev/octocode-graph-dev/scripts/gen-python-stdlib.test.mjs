import assert from 'node:assert/strict';
import { test } from 'node:test';
import { PINNED_PYTHON, TYPESHED_REF, assertPinnedPython, DEFAULT_VERSIONS, removedModules, render } from './gen-python-stdlib.mjs';

test('only top-level modules with an upper bound are removed ones', () => {
  const versions = [
    '# comment',
    'asyncore: 3.0-3.11  # trailing',
    'asyncio: 3.4-',
    'asyncio.mixins: 3.10-3.12',
    'typing_extensions: 3.0-',
    '_bootlocale: 3.4-3.9',
    '',
  ].join('\n');
  assert.deepEqual(removedModules(versions), ['asyncore', '_bootlocale']);
});

test('render sorts, dedupes and names its sources', () => {
  const body = render(['sys', '_io', 'os'], ['os', 'asyncore'], '3.15.2', `typeshed ${TYPESHED_REF}`);
  const names = body.split('\n').filter((line) => line && !line.startsWith('#'));
  assert.deepEqual(names, ['_io', 'asyncore', 'os', 'sys']);
  assert.match(body, /sys\.stdlib_module_names of Python 3\.15\.2/);
});

test('the header states both pins', () => {
  const body = render(['os'], [], `${PINNED_PYTHON}.9`, `typeshed ${TYPESHED_REF}`);
  assert.match(body, new RegExp(`Python ${PINNED_PYTHON}\\.9 \\(pinned line ${PINNED_PYTHON}\\)`));
  assert.match(body, new RegExp(`typeshed ${TYPESHED_REF}`));
  assert.match(DEFAULT_VERSIONS, new RegExp(`/${TYPESHED_REF}/stdlib/VERSIONS$`));
});

test('another Python line is refused, not recorded', () => {
  assertPinnedPython(`${PINNED_PYTHON}.0`);
  const [major, minor] = PINNED_PYTHON.split('.').map(Number);
  assert.throws(() => assertPinnedPython(`${major}.${minor + 1}.0`), /pinned to Python/);
  assert.throws(() => assertPinnedPython(`${major}.${minor - 1}.12`), /pinned to Python/);
});
