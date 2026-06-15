import { chmod, mkdtemp, realpath, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { SymbolResolver, resolveSymbolPosition } from '../src/resolver.js';
import { toUri, fromUri, fromUriSafe } from '../src/uri.js';
import { safeReadFile, validateLSPServerPath } from '../src/validation.js';
import { resolveWorkspaceRootForFile } from '../src/workspaceRoot.js';

const tempDirs: string[] = [];

afterEach(async () => {
  while (tempDirs.length > 0)
    await rm(tempDirs.pop()!, { recursive: true, force: true });
});

async function tempDir(prefix: string): Promise<string> {
  const dir = await mkdtemp(path.join(os.tmpdir(), prefix));
  tempDirs.push(dir);
  return dir;
}

describe('native core wrappers', () => {
  it('converts file URIs through Rust', async () => {
    const file = path.join(await tempDir('octocode-lsp-uri-'), 'a file.ts');
    const uri = toUri(file);
    expect(uri).toMatch(/^file:\/\//);
    expect(fromUri(uri)).toBe(file);
    expect(fromUriSafe(uri)).toEqual({ isValid: true, path: file });
    expect(fromUriSafe('https://example.com/a.ts').isValid).toBe(false);
  });

  it('resolves symbols through Rust', async () => {
    const root = await tempDir('octocode-lsp-symbol-');
    const file = path.join(root, 'demo.ts');
    const content = 'const value = 1;\nfunction target() {}\n';
    await writeFile(file, content);
    const resolver = new SymbolResolver();

    expect(
      resolver.resolvePositionFromContent(content, { symbolName: 'target' })
    ).toMatchObject({ foundAtLine: 2 });
    await expect(resolveSymbolPosition(file, 'target')).resolves.toMatchObject({
      foundAtLine: 2,
    });
  });

  it('uses Rust workspace and validation helpers', async () => {
    const root = await tempDir('octocode-lsp-root-');
    await writeFile(path.join(root, 'package.json'), '{}');
    const file = path.join(root, 'src.ts');
    await writeFile(file, 'export const value = 1;\n');

    await expect(resolveWorkspaceRootForFile(file)).resolves.toBe(root);
    await expect(safeReadFile(file)).resolves.toContain('value');
    await expect(safeReadFile('relative.ts')).resolves.toBeNull();
    expect(validateLSPServerPath(process.execPath)).toEqual({
      isValid: true,
      resolvedPath: await realpath(process.execPath),
    });
    expect(validateLSPServerPath('bash').isValid).toBe(false);
    if (process.platform !== 'win32') {
      const nonExecutable = path.join(root, 'server');
      await writeFile(nonExecutable, 'not executable\n');
      await chmod(nonExecutable, 0o644);
      expect(validateLSPServerPath(nonExecutable).isValid).toBe(false);
    }
  });
});
