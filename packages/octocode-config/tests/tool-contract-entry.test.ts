import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { expect, it } from 'vitest';
import { isEntryPoint } from '../scripts/tool-contract/entry-point.ts';

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const entryPoint = resolve(packageRoot, 'scripts/tool-contract/entry-point.ts');

// A small script through the same check, not the generator: Node's own argv[1] and
// import.meta.url are the behaviour here, and the generator costs seconds to load.
it('a module called through a symlink is its own entry point, and another module is not', () => {
  const dir = mkdtempSync(join(tmpdir(), 'contract-link-'));
  try {
    const script = join(dir, 'script.mjs');
    writeFileSync(
      script,
      `import { isEntryPoint } from ${JSON.stringify(pathToFileURL(entryPoint).href)};\n` +
        'console.log(isEntryPoint(process.argv[1], import.meta.url));\n'
    );
    const link = join(dir, 'link.mjs');
    symlinkSync(script, link);
    for (const path of [script, link]) {
      const run = spawnSync(process.execPath, [path], { encoding: 'utf8' });
      expect(run.stderr).toBe('');
      expect(run.stdout.trim()).toBe('true');
    }
    expect(isEntryPoint(link, pathToFileURL(script).href)).toBe(true);
    expect(isEntryPoint(entryPoint, pathToFileURL(script).href)).toBe(false);
    expect(isEntryPoint(join(dir, 'missing.mjs'), pathToFileURL(script).href)).toBe(false);
    expect(isEntryPoint(undefined, pathToFileURL(script).href)).toBe(false);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

it('the generator runs main through that check', () => {
  // The one line the check above guards; importing the generator would cost seconds.
  const generator = readFileSync(resolve(packageRoot, 'scripts/generate-tool-contract.ts'), 'utf8');
  expect(generator).toMatch(/if \(isEntryPoint\(process\.argv\[1\], import\.meta\.url\)\) main\(\);/);
});
