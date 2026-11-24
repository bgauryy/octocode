// Source maps: the VLQ decoder against hand-encoded segments and a real esbuild map, the map lookup a bundle
// declares (file, data: URL, <bundle>.map), the enclosing named function when no map exists, and `locate` with a graph.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { buildSync } from 'esbuild';
import { tempDir } from './test-tmp.mjs';
import {
  decodeVlq,
  enclosingFunction,
  functionRanges,
  originalPosition,
  parseSourceMap,
  readSourceMap,
} from '../src/runtime/sourcemap.ts';

const B64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
// The encoder the spec describes, so the decoder is checked against segments the test chose.
function vlq(n) {
  let v = n < 0 ? (-n << 1) | 1 : n << 1;
  let out = '';
  do {
    let digit = v & 31;
    v >>>= 5;
    if (v) digit |= 32;
    out += B64[digit];
  } while (v);
  return out;
}
function encode(lines) {
  let src = 0,
    line = 0,
    col = 0,
    name = 0;
  return lines
    .map(segments => {
      let gen = 0;
      return segments
        .map(s => {
          let out = vlq(s[0] - gen);
          gen = s[0];
          if (s.length > 1) {
            out += vlq(s[1] - src) + vlq(s[2] - line) + vlq(s[3] - col);
            [src, line, col] = [s[1], s[2], s[3]];
          }
          if (s.length > 4) {
            out += vlq(s[4] - name);
            name = s[4];
          }
          return out;
        })
        .join(',');
    })
    .join(';');
}

const CPU = `export function checksum(rows) {
  let sum = 0;
  for (const row of rows) sum = (sum * 31 + row) % 1000003;
  return sum + rows.indexOf("needle-in-checksum");
}
export function other(rows) {
  return rows.concat("needle-in-other");
}
`;
const MAIN = `import { checksum, other } from './lib.js';
console.log(checksum([1, 2, 3]), other([]));
`;

// A real minified esbuild bundle of main.js → lib.js, written with the map style asked for. The directory is a real
// path, as esbuild writes its sources relative to real paths.
function bundle(dir, sourcemap) {
  mkdirSync(join(dir, 'src'), { recursive: true });
  writeFileSync(join(dir, 'src/lib.js'), CPU);
  writeFileSync(join(dir, 'src/main.js'), MAIN);
  const out = join(dir, 'dist/out.js');
  const result = buildSync({
    entryPoints: [join(dir, 'src/main.js')],
    bundle: true,
    minify: true,
    format: 'esm',
    sourcemap,
    outfile: out,
    write: false,
    logLevel: 'silent',
  });
  mkdirSync(join(dir, 'dist'), { recursive: true });
  for (const f of result.outputFiles) writeFileSync(f.path, f.contents);
  return out;
}
const positionOf = (text, needle) => {
  const at = text.indexOf(needle);
  const before = text.slice(0, at).split('\n');
  return { line: before.length, column: before.at(-1).length + 1 };
};

test('VLQ: sign bit, continuation digits and multi-digit values decode as the spec encodes them', () => {
  assert.deepEqual(decodeVlq('A'), [0]);
  assert.deepEqual(decodeVlq('D'), [-1]);
  assert.deepEqual(decodeVlq('gB'), [16]);
  assert.deepEqual(decodeVlq('2H'), [123]);
  assert.deepEqual(decodeVlq('AAgBC'), [0, 0, 16, 1]);
  for (const n of [0, 1, -1, 15, 16, -16, 1023, -1024, 123456789])
    assert.deepEqual(decodeVlq(vlq(n)), [n]);
});

test('lookup: the greatest segment at or before the column, names, source roots and unmapped segments', () => {
  const map = parseSourceMap(
    {
      version: 3,
      sourceRoot: 'src',
      sources: ['a.ts', 'b.ts'],
      names: ['alpha', 'beta'],
      mappings: encode([
        [[0, 0, 0, 0, 0], [10, 1, 4, 2], [20]],
        [],
        [[3, 0, 9, 6, 1]],
      ]),
    },
    '/build/dist'
  );
  assert.equal(map.sources[0], '/build/dist/src/a.ts');
  assert.deepEqual(originalPosition(map, 1, 1), { source: '/build/dist/src/a.ts', line: 1, column: 1, name: 'alpha' });
  assert.deepEqual(originalPosition(map, 1, 15), { source: '/build/dist/src/b.ts', line: 5, column: 3, name: null });
  // A segment with only a generated column maps nothing: no guess from the segment before it.
  assert.equal(originalPosition(map, 1, 25), null);
  assert.equal(originalPosition(map, 2, 1), null);
  assert.deepEqual(originalPosition(map, 3, 50), { source: '/build/dist/src/a.ts', line: 10, column: 7, name: 'beta' });
  assert.equal(originalPosition(map, 3, 2), null);
  assert.equal(originalPosition(map, 9, 1), null);
});

test('a real esbuild map: each minified position maps to its original line, by file comment, data: URL or <bundle>.map', () => {
  for (const style of ['external', 'inline', 'linked']) {
    const dir = realpathSync(tempDir('graph-sourcemap-'));
    const out = bundle(dir, style);
    let text = readFileSync(out, 'utf8');
    if (style === 'external') assert.doesNotMatch(text, /sourceMappingURL/);
    const found = readSourceMap(out, text);
    assert.equal(found.ok, true, found.reason);
    assert.equal(found.mapFile, style === 'inline' ? 'inline' : `${out}.map`);
    assert.match(found.sha256, /^[\da-f]{64}$/);
    for (const [needle, line] of [
      ['"needle-in-checksum"', 4],
      ['"needle-in-other"', 7],
    ]) {
      const at = positionOf(text, needle);
      const mapped = originalPosition(found.map, at.line, at.column);
      assert.equal(mapped.source, join(dir, 'src/lib.js'), style);
      assert.equal(mapped.line, line, `${style} ${needle}`);
    }
    text = '';
  }
});

test('a bundle without a map says why; a named map that is missing is not replaced by a guess', () => {
  const dir = tempDir('graph-sourcemap-none-');
  writeFileSync(join(dir, 'plain.js'), 'function a(){}\n');
  const none = readSourceMap(join(dir, 'plain.js'), 'function a(){}\n');
  assert.equal(none.ok, false);
  assert.match(none.reason, /no sourceMappingURL comment and no .*plain\.js\.map/);
  const named = 'function a(){}\n//# sourceMappingURL=gone.js.map\n';
  writeFileSync(join(dir, 'named.js'), named);
  const missing = readSourceMap(join(dir, 'named.js'), named);
  assert.equal(missing.ok, false);
  assert.match(missing.reason, /gone\.js\.map/);
});

test('no map: the innermost named function around a position, past strings, regexes, templates and comments', () => {
  const text = [
    'const s = "{ not a brace }"; var re = /[{}]\\//g; const t = `x ${ {a:1}.a } }y`; // {',
    'function outer(a) { /* } */ function inner(b) { return b + "}"; } return inner(a); }',
    'const arrow = (x) => { return x * 2; };',
    'obj = { method(y) { if (y) { return y; } }, prop: function () { return 1; }, [k]() { return 2; } };',
    'export const later = async z => { return z; };',
    'top();',
  ].join('\n');
  const ranges = functionRanges(text);
  const at = needle => enclosingFunction(ranges, text.indexOf(needle));
  assert.equal(at('return b').name, 'inner');
  assert.equal(at('return inner').name, 'outer');
  assert.equal(at('return x * 2').name, 'arrow');
  assert.equal(at('return y').name, 'method');
  assert.equal(at('return 1').name, 'prop');
  assert.equal(at('return z').name, 'later');
  // A computed method has no name of its own: the answer says so instead of borrowing a neighbor's.
  assert.equal(at('return 2'), null);
  assert.equal(at('top()'), null);
  // V8 places a function at its parameter list: that position belongs to the function.
  assert.equal(enclosingFunction(ranges, text.indexOf('(b)')).name, 'inner');
});

const graph = (dir, symbols) => {
  const path = join(dir, 'data.json');
  writeFileSync(
    path,
    JSON.stringify({
      section: dir,
      files: [{ id: 'src/lib.js' }, { id: 'src/main.js' }],
      symbols,
      imports: [],
      calls: [],
    })
  );
  return path;
};
const locate = (...args) =>
  spawnSync(process.execPath, [new URL('../src/runtime/commands/locate.ts', import.meta.url).pathname, ...args], {
    encoding: 'utf8',
  });

test('locate: original position, name and the enclosing graph symbol with its declaration line', () => {
  const dir = realpathSync(tempDir('graph-locate-'));
  const out = bundle(dir, 'linked');
  const text = readFileSync(out, 'utf8');
  const data = graph(dir, [
    { id: 'src/lib.js#checksum', file: 'src/lib.js', name: 'checksum', kind: 'function', line: 1, endLine: 5 },
    { id: 'src/lib.js#other', file: 'src/lib.js', name: 'other', kind: 'function', line: 6, endLine: 8 },
  ]);
  const at = positionOf(text, '"needle-in-checksum"');
  const r = locate(`${out}:${at.line}:${at.column}`, '--graph', data, '--json');
  assert.equal(r.status, 0, r.stderr);
  const [row] = JSON.parse(r.stdout);
  assert.equal(row.original.path, 'src/lib.js');
  assert.equal(row.original.line, 4);
  assert.deepEqual(row.symbol, { id: 'src/lib.js#checksum', name: 'checksum', kind: 'function', line: 1, endLine: 5, resolvedBy: 'position' });
  const plain = locate(`${out}:${at.line}:${at.column}`, '--graph', data);
  assert.match(plain.stdout, /→ src\/lib\.js:4:\d+ .*in src\/lib\.js#checksum \(declared at line 1\)/);
  // A position past the end of the bundle is an answer that says so, not a failure of the whole command.
  const far = locate(`${out}:999:1`, '--json');
  assert.equal(far.status, 0);
  assert.match(JSON.parse(far.stdout)[0].reason, /line 999/);
  assert.equal(locate('nope.js:1:1').status, 1);
  assert.equal(locate('nope.js').status, 2);
});

test('locate without a map: the generated function name, resolved against graph symbols, ambiguity as candidates', () => {
  const dir = tempDir('graph-locate-nomap-');
  const bundled = join(dir, 'bundle.js');
  writeFileSync(bundled, 'function inner(a){return a+1}\nfunction checksum(r){return r}\n');
  const data = graph(dir, [
    { id: 'src/lib.js#checksum', file: 'src/lib.js', name: 'checksum', kind: 'function', line: 1, endLine: 5 },
    { id: 'src/lib.js#inner', file: 'src/lib.js', name: 'inner', kind: 'function', line: 6, endLine: 8 },
    { id: 'src/main.js#inner', file: 'src/main.js', name: 'inner', kind: 'function', line: 2, endLine: 3 },
  ]);
  const r = locate(`${bundled}:1:20`, `${bundled}:2:25`, '--graph', data, '--json');
  assert.equal(r.status, 0, r.stderr);
  const [ambiguous, unique] = JSON.parse(r.stdout);
  assert.equal(ambiguous.original, null);
  assert.match(ambiguous.reason, /no sourceMappingURL/);
  assert.deepEqual(ambiguous.generated, { name: 'inner', candidates: ['src/lib.js#inner', 'src/main.js#inner'] });
  assert.equal(ambiguous.symbol, null);
  assert.deepEqual(unique.generated, { name: 'checksum', candidates: ['src/lib.js#checksum'] });
  assert.equal(unique.symbol.id, 'src/lib.js#checksum');
  assert.equal(unique.symbol.resolvedBy, 'name');
});
