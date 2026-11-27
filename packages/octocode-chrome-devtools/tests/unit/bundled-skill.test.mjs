import test from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';

const root = resolve(import.meta.dirname, '../..');
const skill = join(root, 'skills/octocode-chrome-devtools');
const pkg = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'));

/** The files npm would publish, from npm itself (no scripts run, nothing written). */
const packed = () =>
  JSON.parse(
    execFileSync('npm', ['pack', '--dry-run', '--ignore-scripts', '--json'], {
      cwd: root,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    })
  )[0].files.map(file => file.path);

const sourceFiles = readdirSync(skill, { recursive: true, withFileTypes: true })
  .filter(entry => entry.isFile())
  .map(entry =>
    relative(root, join(entry.parentPath, entry.name)).replaceAll('\\', '/')
  );

test('the npm archive ships every skill file from its one source, with no second copy in dist', () => {
  const files = packed();
  assert.ok(sourceFiles.includes('skills/octocode-chrome-devtools/SKILL.md'));
  for (const file of sourceFiles)
    assert.ok(files.includes(file), `not packed: ${file}`);
  assert.deepEqual(
    files.filter(file => file.startsWith('dist/skills')),
    []
  );
});

test('the ./skill export resolves SKILL.md inside the packed files', () => {
  const target = pkg.exports['./skill'];
  assert.ok(existsSync(join(root, target)), target);
  assert.ok(packed().includes(target.replace(/^\.\//, '')));
});

test('every relative link inside the packed skill resolves within the packed skill', () => {
  const files = new Set(packed());
  for (const file of sourceFiles.filter(name => name.endsWith('.md')))
    for (const link of readFileSync(join(root, file), 'utf8').matchAll(
      /\]\(((?!https?:|#)[^)#\s]+)/g
    )) {
      const target = relative(
        root,
        resolve(root, dirname(file), link[1])
      ).replaceAll('\\', '/');
      assert.ok(files.has(target), `${file} links ${link[1]}`);
    }
});
