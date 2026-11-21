import test from 'node:test';
import assert from 'node:assert/strict';
import { isAbsolute, join, relative, sep } from 'node:path';
import {
  originalPositionFor,
  parseSourceMap,
  readScriptMapText,
} from '../../dist/engine/sourcemap-resolver.mjs';
import {
  confinedPath,
  fileStem,
  sanitizeSourcePath,
} from '../../dist/engine/cdp-checks/script-source-paths.mjs';
import { handMap } from './source-map-fixture.mjs';
import { parsePositions } from '../../dist/engine/cdp-checks/source-map-check.mjs';

test('forward lookup maps known generated positions to known originals', () => {
  const parsed = parseSourceMap(JSON.stringify(handMap));
  // generated line is 0-based here; the recipe passes line - 1.
  assert.deepEqual(originalPositionFor(parsed, 0, 0), {
    source: 'src/a.ts',
    line: 1,
    col: 0,
    name: null,
  });
  assert.deepEqual(originalPositionFor(parsed, 0, 12), {
    source: 'src/a.ts',
    line: 3,
    col: 4,
    name: 'foo',
  });
  assert.deepEqual(originalPositionFor(parsed, 1, 3), {
    source: 'src/b.ts',
    line: 6,
    col: 2,
    name: null,
  });
  assert.equal(originalPositionFor(parsed, 2, 0), null);
});

test('malformed maps are named, not decoded', () => {
  assert.throws(() => parseSourceMap('nope'), /Malformed.*JSON/);
  assert.throws(() => parseSourceMap('{"sources":[]}'), /Malformed/);
  assert.throws(
    () => parseSourceMap('{"sources":["a"],"mappings":"!!"}'),
    /Malformed/
  );
  assert.throws(() => parseSourceMap('{"sections":[]}'), /indexed/);
});

// A refused or local map never reaches the session.
const noSession = {
  send() {
    throw new Error('session must not be used');
  },
};

test('data: URI maps decode locally; a foreign-origin map is refused without a fetch', async () => {
  const text = JSON.stringify(handMap);
  const b64 = `data:application/json;base64,${Buffer.from(text).toString('base64')}`;
  assert.equal(
    await readScriptMapText(noSession, b64, 'http://127.0.0.1:1/a.js'),
    text
  );
  const plain = `data:application/json,${encodeURIComponent(text)}`;
  assert.equal(await readScriptMapText(noSession, plain, ''), text);
  await assert.rejects(
    readScriptMapText(
      noSession,
      'http://other.invalid/a.map',
      'http://127.0.0.1:1/a.js'
    ),
    /Refused.*origin/
  );
  await assert.rejects(readScriptMapText(noSession, 'a.js.map', ''), /Refused/);
});

test('saved source paths can never escape the output dir', () => {
  const root = join('/tmp', 'out', 'sources');
  const hostile = [
    '../../etc/passwd',
    '/etc/passwd',
    'webpack://app/../../../secret',
    'C:\\Windows\\system32\\x',
    '..\\..\\boot.ini',
    'file:///etc/shadow',
    '....//x',
    '',
    '..',
  ];
  hostile.forEach((source, i) => {
    const safe = sanitizeSourcePath(source, i);
    assert.ok(!isAbsolute(safe), safe);
    assert.ok(!safe.split('/').includes('..'), safe);
    const target = confinedPath(root, safe);
    assert.ok(target, safe);
    const rel = relative(root, target);
    assert.ok(!rel.startsWith('..') && !isAbsolute(rel), `${source} -> ${rel}`);
  });
  assert.equal(confinedPath(root, '../x'), null);
  assert.equal(confinedPath(root, '..'), null);
  assert.ok(fileStem('http://h/a/b/app.js?x=1', 'f') === 'app.js');
  assert.ok(!fileStem('../..', 'fb').includes(sep));
});

test('position lists parse l:c pairs and report bad entries', () => {
  assert.deepEqual(parsePositions('1:0, 3:14'), {
    ok: [
      { line: 1, column: 0 },
      { line: 3, column: 14 },
    ],
    bad: [],
  });
  assert.deepEqual(parsePositions('0:1,x,2-3').bad, ['0:1', 'x', '2-3']);
});
