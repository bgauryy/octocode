// Benchmark scripts must run when called through a symlink (installed copies, and macOS /tmp
// and /var are symlinks too): argv[1] keeps the link while import.meta.url is the real path.
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readdirSync, readFileSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const pkg = fileURLToPath(new URL('..', import.meta.url));
const helper = resolve(pkg, '../../skills/common/scripts/lib/entrypoint.mjs');
const modules = ['scripts', 'local/e2e/harness']
  .flatMap(dir => readdirSync(join(pkg, dir), { recursive: true }).map(file => join(pkg, dir, file)))
  .filter(file => file.endsWith('.mjs') && !file.endsWith('.test.mjs'));
// Entry guards that compare argv[1] with the module URL break through a symlink.
const RAW_GUARD = /import\.meta\.main|require\.main|(?:pathToFileURL|realpathSync)\(\s*process\.argv\[1\]|process\.argv\[1\]\s*===|===\s*process\.argv\[1\]/;

test('every entry guard goes through the shared isMainModule helper', () => {
  const guarded = [];
  for (const file of modules) {
    const source = readFileSync(file, 'utf8');
    const name = relative(pkg, file);
    assert.doesNotMatch(source, RAW_GUARD, `${name} compares argv[1] directly; use isMainModule`);
    const imported = /import \{ isMainModule \} from '([^']+)'/.exec(source)?.[1];
    if (!imported) continue;
    assert.equal(resolve(dirname(file), imported), helper, `${name} imports isMainModule from the wrong path`);
    assert.match(source, /^if \(isMainModule\(import\.meta\.url\)/m, `${name} imports isMainModule without guarding its entry`);
    guarded.push(name);
  }
  assert.ok(guarded.includes('scripts/classification_proxy.mjs'), `the symlink sample below must be guarded: ${guarded.join(", ")}`);
});

test('a guarded script runs its entry point when called through a symlink', t => {
  const dir = mkdtempSync(join(tmpdir(), 'bench-link-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const link = join(dir, 'classification_proxy.mjs');
  symlinkSync(join(pkg, 'scripts/classification_proxy.mjs'), link);
  const run = spawnSync(process.execPath, [link], { encoding: 'utf8', env: { PATH: process.env.PATH } });
  assert.match(`${run.stdout}${run.stderr}`, /Usage: classification_proxy\.mjs --log/, `nothing ran (exit ${run.status})`);
});
