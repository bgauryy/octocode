import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { importsOf, packageName, stripComments } from '../deps-usage.mjs';

const script = fileURLToPath(new URL('../deps-usage.mjs', import.meta.url));

function tree(files) {
  const dir = mkdtempSync(join(tmpdir(), 'deps-usage-'));
  for (const [name, content] of Object.entries(files)) {
    mkdirSync(dirname(join(dir, name)), { recursive: true });
    writeFileSync(join(dir, name), typeof content === 'string' ? content : JSON.stringify(content));
  }
  return dir;
}

function run(args, cwd) {
  const result = spawnSync(process.execPath, [script, ...args], { cwd, encoding: 'utf8', env: { PATH: process.env.PATH } });
  return { status: result.status, json: JSON.parse(result.stdout) };
}

const INDEX = [
  "import used from 'used';",
  "import { readFile } from 'node:fs';",
  "import type { T } from '@scope/lib/types';",
  "// require('ghost') in a comment",
  "/* import x from 'ghost2' */",
  'const url = "http://example.com//not-a-comment";',
  "const lazy = () => import('hoisted-x');",
  "const r = require('undeclared-y');",
  "export { react } from 'react';",
  "import self from '@me/pkg/sub';",
  "import rel from './local.js';",
  "import dup from 'dup';",
].join('\n');

test('reports unimported, undeclared, test-only, doubly declared and referenced-only dependencies', () => {
  const root = tree({
    'package.json': { name: 'mono', devDependencies: { 'hoisted-x': '1' } },
    'pkg/package.json': {
      name: '@me/pkg',
      scripts: { test: 'vitest run', build: 'tb --fast' },
      dependencies: { used: '1', unused: '1', testonly: '1', dup: '1', '@scope/lib': '1', react: '18' },
      devDependencies: { vitest: '1', dup: '1', '@types/used': '1', '@types/node': '1', 'eslint-plugin-foo': '1', 'not-installed-tool': '1', 'tool-bin': '1' },
      peerDependencies: { react: '18' },
    },
    'pkg/src/index.ts': `${INDEX}\n`,
    'pkg/src/env.d.ts': '/// <reference types="node" />\n',
    'pkg/test/index.test.ts': "import t from 'testonly';\nimport { it } from 'vitest';\n",
    'pkg/sub/package.json': { name: 'sub' },
    'pkg/sub/x.js': "import nested from 'nested-only';\n",
    'pkg/.eslintrc.json': { plugins: ['eslint-plugin-foo'] },
    'pkg/node_modules/unused/package.json': { name: 'unused', bin: { 'unused-cli': 'cli.js' } },
    'pkg/node_modules/tool-bin/package.json': { name: 'tool-bin', bin: { tb: 'cli.js' } },
  });
  try {
    const { status, json } = run(['pkg'], root);
    assert.equal(status, 1);
    assert.equal(json.ok, false);
    assert.equal(json.package, '@me/pkg');
    assert.deepEqual(json.unimported, [
      { name: 'not-installed-tool', section: 'devDependencies', binsChecked: false },
      { name: 'unused', section: 'dependencies', binsChecked: true },
    ]);
    assert.deepEqual(json.undeclared, [
      { name: 'hoisted-x', sites: [{ file: 'src/index.ts', line: 7 }], declaredBy: '../package.json' },
      { name: 'undeclared-y', sites: [{ file: 'src/index.ts', line: 8 }] },
    ]);
    assert.deepEqual(json.testOnlyInDependencies, [{ name: 'testonly', sites: [{ file: 'test/index.test.ts', line: 1 }] }]);
    assert.deepEqual(json.inMultipleSections, [
      { name: 'dup', sections: ['dependencies', 'devDependencies'] },
      { name: 'react', sections: ['dependencies', 'peerDependencies'] },
    ]);
    assert.deepEqual(json.referencedOnly, [
      { name: 'eslint-plugin-foo', section: 'devDependencies', in: ['.eslintrc.json'] },
      { name: 'tool-bin', section: 'devDependencies', in: ['package.json scripts'] },
    ]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('exits 0 when every declared dependency is imported and every import is declared', () => {
  const root = tree({ 'package.json': { name: 'ok', dependencies: { a: '1' } }, 'index.js': "const a = require('a');\n" });
  try {
    const { status, json } = run(['.'], root);
    assert.equal(status, 0);
    assert.equal(json.ok, true);
    assert.equal(json.filesScanned, 1);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('a folder without package.json is invalid input', () => {
  const root = tree({ 'index.js': '' });
  try {
    const { status, json } = run(['.'], root);
    assert.equal(status, 2);
    assert.equal(json.error.code, 'no-manifest');
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('comment stripping keeps strings and line numbers', () => {
  const text = 'a // gone\n"keep // this" /* x\ny */ b\n';
  const stripped = stripComments(text);
  assert.equal(stripped.split('\n').length, text.split('\n').length);
  assert.ok(stripped.includes('"keep // this"'));
  assert.ok(!stripped.includes('gone') && !stripped.includes('x'));
});

test('package names drop subpaths and skip relative, subpath-import, scheme and builtin specifiers', () => {
  assert.equal(packageName('@scope/lib/deep/file'), '@scope/lib');
  assert.equal(packageName('lodash/fp'), 'lodash');
  for (const specifier of ['./x', '../y', '/abs', '#internal', 'node:fs', 'fs/promises', 'https://cdn/x.js', 'data:text/javascript,1']) assert.equal(packageName(specifier), null, specifier);
  assert.deepEqual(importsOf("import {\n  a,\n  b,\n} from 'multi';\n").imports, [{ name: 'multi', line: 4 }]);
});
