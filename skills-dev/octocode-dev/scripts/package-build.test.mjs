import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, relative } from 'node:path';
import { REPO_BLOB_URL, stagePackage } from './package-build.mjs';

const files = dir => readdirSync(dir, { withFileTypes: true, recursive: true })
  .filter(entry => entry.isFile())
  .map(entry => relative(dir, join(entry.parentPath, entry.name)))
  .sort();

// A synthetic repository, so staging never writes into the shared tree.
function repo() {
  const root = mkdtempSync(join(tmpdir(), 'package-stage-'));
  mkdirSync(join(root, 'skills', 'alpha', 'references'), { recursive: true });
  writeFileSync(join(root, 'skills', 'alpha', 'SKILL.md'), 'alpha skill');
  writeFileSync(join(root, 'skills', 'alpha', 'references', 'guide.md'), 'guide');
  mkdirSync(join(root, 'docs', 'guides'), { recursive: true });
  writeFileSync(join(root, 'docs', 'README.md'), '[tools](guides/TOOLS.md) and [package](../packages/x/README.md#setup)');
  writeFileSync(join(root, 'docs', 'guides', 'TOOLS.md'), 'tools');
  const pkg = join(root, 'packages', 'p');
  mkdirSync(join(pkg, 'skills', 'removed'), { recursive: true });
  writeFileSync(join(pkg, 'skills', 'removed', 'SKILL.md'), 'stale skill');
  mkdirSync(join(pkg, 'dist', 'docs'), { recursive: true });
  writeFileSync(join(pkg, 'dist', 'docs', 'OLD.md'), 'stale doc');
  return { root, pkg };
}

test('stagePackage ships every repo skill and doc into the package and drops stale copies', () => {
  const { root, pkg } = repo();
  try {
    const staged = stagePackage(pkg, 'dist', root);
    assert.deepEqual(staged, { skills: join(pkg, 'skills'), docs: join(pkg, 'dist', 'docs') });
    assert.deepEqual(files(staged.skills), files(join(root, 'skills')));
    assert.equal(readFileSync(join(staged.skills, 'alpha', 'SKILL.md'), 'utf8'), 'alpha skill');
    assert.equal(existsSync(join(staged.skills, 'removed')), false, 'a skill gone from the repo leaves the package');
    assert.deepEqual(files(staged.docs), files(join(root, 'docs')));
    assert.equal(existsSync(join(staged.docs, 'OLD.md')), false, 'a doc gone from the repo leaves the package');
    const readme = readFileSync(join(staged.docs, 'README.md'), 'utf8');
    assert.ok(readme.includes('](guides/TOOLS.md)'), 'a link inside docs/ stays relative');
    assert.ok(readme.includes(`](${REPO_BLOB_URL}packages/x/README.md#setup)`), 'a link leaving docs/ points at the repository');
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('stagePackage refuses a repository without docs instead of shipping none', () => {
  const { root, pkg } = repo();
  try {
    rmSync(join(root, 'docs'), { recursive: true, force: true });
    assert.throws(() => stagePackage(pkg, 'dist', root), /documentation directory is missing/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('both published packages stage skills and docs when packed', () => {
  const root = join(import.meta.dirname, '..', '..', '..');
  const read = path => JSON.parse(readFileSync(join(root, path), 'utf8'));
  const mcp = read('packages/octocode-mcp/package.json');
  assert.ok(mcp.files.includes('skills/**'), 'octocode-mcp files list the staged skills');
  assert.ok(mcp.files.includes('dist/docs/**'), 'octocode-mcp files list the staged docs');
  assert.match(mcp.scripts.prepack, /package-build\.mjs stage dist\b/);
  const cli = read('packages/octocode/package.json');
  assert.ok(cli.files.includes('skills/**'), 'octocode files list the staged skills');
  assert.ok(cli.files.includes('out'), 'octocode files list out/, which holds out/docs');
  assert.match(readFileSync(join(root, 'packages/octocode/scripts/prepack.mjs'), 'utf8'), /stagePackage\([^)]*'out'/);
});
