import { mkdtemp, mkdir, rm, writeFile } from 'fs/promises';
import os from 'os';
import path from 'path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import {
  findWorkspaceRoot,
  resolveWorkspaceRootForFile,
} from '../../src/lsp/workspaceRoot.js';

describe('LSP workspace root resolution', () => {
  let tempDir: string;

  beforeEach(async () => {
    tempDir = await mkdtemp(path.join(os.tmpdir(), 'octocode-lsp-root-'));
    delete process.env.WORKSPACE_ROOT;
  });

  afterEach(async () => {
    delete process.env.WORKSPACE_ROOT;
    await rm(tempDir, { recursive: true, force: true });
  });

  it('prefers the configured workspace root when the file is inside it', async () => {
    const repoRoot = path.join(tempDir, 'repo');
    const filePath = path.join(repoRoot, 'src', 'index.ts');

    await mkdir(path.dirname(filePath), { recursive: true });
    await writeFile(path.join(repoRoot, 'package.json'), '{}');
    await writeFile(filePath, 'export const value = 1;\n');

    process.env.WORKSPACE_ROOT = repoRoot;

    await expect(resolveWorkspaceRootForFile(filePath)).resolves.toBe(repoRoot);
  });

  it('infers the nearest project root when WORKSPACE_ROOT points elsewhere', async () => {
    const configuredRoot = path.join(tempDir, 'configured-root');
    const repoRoot = path.join(tempDir, 'external-repo');
    const filePath = path.join(repoRoot, 'packages', 'app', 'src', 'index.ts');

    await mkdir(configuredRoot, { recursive: true });
    await mkdir(path.dirname(filePath), { recursive: true });
    await writeFile(path.join(repoRoot, 'package.json'), '{}');
    await writeFile(filePath, 'export const value = 1;\n');

    process.env.WORKSPACE_ROOT = configuredRoot;

    await expect(resolveWorkspaceRootForFile(filePath)).resolves.toBe(repoRoot);
  });

  it('falls back to the file directory when no workspace markers exist', async () => {
    const filePath = path.join(tempDir, 'loose', 'nested', 'script.js');

    await mkdir(path.dirname(filePath), { recursive: true });
    await writeFile(filePath, 'console.log("hello");\n');

    await expect(findWorkspaceRoot(filePath)).resolves.toBe(
      path.dirname(filePath)
    );
  });

  it('returns markerRoot when configuredRoot equals markerRoot (short-circuit branch)', async () => {
    // Covers the `configuredRoot !== markerRoot` false-branch in resolveWorkspaceRootForFile.
    // cwd() IS the marker root (both the same dir) — the condition is false, returns markerRoot.
    const repoRoot = path.join(tempDir, 'same-root');
    const filePath = path.join(repoRoot, 'src', 'index.ts');

    await mkdir(path.dirname(filePath), { recursive: true });
    await writeFile(path.join(repoRoot, 'package.json'), '{}');
    await writeFile(filePath, '');

    // Point WORKSPACE_ROOT at exactly the same dir that has package.json.
    process.env.WORKSPACE_ROOT = repoRoot;

    // configuredRoot === markerRoot (both resolve to repoRoot), so the early-return
    // branch is skipped and markerRoot (== repoRoot) is returned.
    await expect(resolveWorkspaceRootForFile(filePath)).resolves.toBe(repoRoot);
  });

  it('can infer a package root for files inside node_modules bundles', async () => {
    const packageRoot = path.join(
      tempDir,
      'repo',
      'node_modules',
      '@anthropic-ai',
      'claude-code'
    );
    const filePath = path.join(packageRoot, 'cli.js');

    await mkdir(packageRoot, { recursive: true });
    await writeFile(path.join(packageRoot, 'package.json'), '{}');
    await writeFile(filePath, 'console.log("bundled");\n');

    await expect(resolveWorkspaceRootForFile(filePath)).resolves.toBe(
      packageRoot
    );
  });
});
