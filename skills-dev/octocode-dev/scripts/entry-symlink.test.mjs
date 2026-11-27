import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const scripts = fileURLToPath(new URL('.', import.meta.url));

// Each script's cheapest side-effect-free mode.
const cases = [
  ['prepublish.mjs', /Prepublish check/],
  ['package-build.mjs', /Usage: node package-build\.mjs docs/],
];

for (const [script, expected] of cases) {
  test(`${script} runs its entry point when called through a symlink`, () => {
    const dir = mkdtempSync(join(tmpdir(), 'dev-link-'));
    try {
      const link = join(dir, script);
      symlinkSync(join(scripts, script), link);
      const r = spawnSync(process.execPath, [link], { encoding: 'utf8', env: { PATH: '' } });
      assert.match(`${r.stdout}${r.stderr}`, expected, `${script} did nothing through a symlink (exit ${r.status})`);
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });
}

test('a linked formula generator requests the pinned version', (t) => {
  const dir = mkdtempSync(join(tmpdir(), 'formula-link-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const link = join(dir, 'brew-formula.mjs');
  symlinkSync(join(scripts, 'brew-formula.mjs'), link);
  const mock = join(dir, 'registry.mjs');
  writeFileSync(mock, 'globalThis.fetch = async url => { console.log(url); process.exit(23); };');
  const result = spawnSync(process.execPath, ['--import', mock, link, '1.2.3'], { cwd: dir, encoding: 'utf8' });
  assert.equal(result.status, 23, result.stderr);
  assert.equal(new URL(result.stdout.trim()).pathname.split('/').at(-1), '1.2.3');
});
