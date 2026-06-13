import { existsSync } from 'node:fs';
import { mkdtemp, mkdir, rm, symlink, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { attachLspEvidence } from '../src/evidence.js';
import { buildInitializeParams } from '../src/initParams.js';
import { buildChildProcessEnv } from '../src/processEnv.js';
import { SymbolResolver, SymbolResolutionError } from '../src/resolver.js';
import { LSPConfigFileSchema } from '../src/schemas.js';
import { convertSymbolKind, toLSPSymbolKind } from '../src/symbols.js';
import { fromUri, fromUriSafe, toUri, UnsafeUriError } from '../src/uri.js';
import { safeReadFile, validateLSPServerPath } from '../src/validation.js';
import {
  findWorkspaceRoot,
  resolveWorkspaceRootForFile,
} from '../src/workspaceRoot.js';
import { SymbolKind } from 'vscode-languageserver-protocol';
import { resolveSymbolPosition } from '../src/resolver.js';
import type { SymbolKind as OctocodeSymbolKind } from '../src/types.js';

const tempDirs: string[] = [];
const TEST_ROOT = path.join(os.homedir(), 'octocode-lsp-test-workspaces');

afterEach(async () => {
  while (tempDirs.length > 0) {
    await rm(tempDirs.pop()!, { recursive: true, force: true });
  }
});

async function tempDir(prefix: string): Promise<string> {
  await mkdir(TEST_ROOT, { recursive: true });
  const dir = await mkdtemp(path.join(TEST_ROOT, prefix));
  tempDirs.push(dir);
  return dir;
}

describe('core helpers', () => {
  it('builds initialize params with TS defaults and custom options', async () => {
    const root = await tempDir('octocode-lsp-init-');
    const params = buildInitializeParams({
      command: 'server',
      args: [],
      workspaceRoot: root,
      languageId: 'typescript',
      initializationOptions: { preferences: { quotePreference: 'single' } },
    });

    expect(params.rootUri).toBe(toUri(root));
    expect(params.workspaceFolders?.[0]?.name).toBe(path.basename(root));
    expect(params.initializationOptions).toMatchObject({
      tsserver: expect.any(Object),
      preferences: { quotePreference: 'single' },
    });
    expect(params.capabilities.textDocument?.definition).toMatchObject({
      linkSupport: true,
    });
  });

  it('filters child process env to the allowlist', () => {
    const oldPath = process.env.PATH;
    const oldSecret = process.env.GITHUB_TOKEN;
    process.env.PATH = '/bin';
    process.env.GITHUB_TOKEN = 'secret';

    const env = buildChildProcessEnv(
      { PATH: '/custom/bin', TMPDIR: undefined, GITHUB_TOKEN: 'leak' },
      ['PATH', 'TMPDIR']
    );

    expect(env.PATH).toBe('/custom/bin');
    expect(env.TMPDIR).toBeUndefined();
    expect(env.GITHUB_TOKEN).toBeUndefined();

    process.env.PATH = oldPath;
    if (oldSecret === undefined) delete process.env.GITHUB_TOKEN;
    else process.env.GITHUB_TOKEN = oldSecret;
  });

  it('converts URIs safely', async () => {
    const file = path.join(await tempDir('octocode-lsp-uri-'), 'a file.ts');
    const uri = toUri(file);

    expect(uri).toMatch(/^file:\/\//);
    expect(toUri(uri)).toBe(uri);
    expect(fromUri(uri)).toBe(file);
    expect(fromUri('not-a-uri')).toBe('not-a-uri');
    expect(fromUriSafe(uri)).toEqual({ isValid: true, path: file });
    expect(fromUriSafe('')).toMatchObject({
      isValid: false,
      error: 'uri must be a non-empty string',
    });
    expect(fromUriSafe('/tmp/file.ts')).toMatchObject({
      isValid: false,
      error: 'uri is missing a scheme',
    });
    expect(fromUriSafe(`file://${file}\u0000x`)).toMatchObject({
      isValid: false,
      error: 'uri contains null byte',
    });
    expect(fromUriSafe('https://example.com/a.ts').isValid).toBe(false);
    expect(() =>
      fromUriSafe('https://example.com/a.ts', { throwOnInvalid: true })
    ).toThrow(UnsafeUriError);
  });

  it('maps LSP symbol kinds in both directions', () => {
    const fromLsp: Array<[SymbolKind, OctocodeSymbolKind]> = [
      [SymbolKind.Function, 'function'],
      [SymbolKind.Method, 'method'],
      [SymbolKind.Constructor, 'method'],
      [SymbolKind.Class, 'class'],
      [SymbolKind.Struct, 'class'],
      [SymbolKind.Interface, 'interface'],
      [SymbolKind.Variable, 'variable'],
      [SymbolKind.Constant, 'constant'],
      [SymbolKind.Property, 'property'],
      [SymbolKind.Field, 'property'],
      [SymbolKind.Enum, 'enum'],
      [SymbolKind.EnumMember, 'constant'],
      [SymbolKind.Module, 'module'],
      [SymbolKind.Package, 'module'],
      [SymbolKind.File, 'module'],
      [SymbolKind.Namespace, 'namespace'],
      [SymbolKind.TypeParameter, 'type'],
    ];

    for (const [lspKind, octocodeKind] of fromLsp) {
      expect(convertSymbolKind(lspKind)).toBe(octocodeKind);
    }

    const toLsp: Array<[OctocodeSymbolKind, SymbolKind]> = [
      ['function', SymbolKind.Function],
      ['method', SymbolKind.Method],
      ['class', SymbolKind.Class],
      ['interface', SymbolKind.Interface],
      ['variable', SymbolKind.Variable],
      ['constant', SymbolKind.Constant],
      ['property', SymbolKind.Property],
      ['enum', SymbolKind.Enum],
      ['module', SymbolKind.Module],
      ['namespace', SymbolKind.Namespace],
      ['type', SymbolKind.TypeParameter],
    ];

    for (const [octocodeKind, lspKind] of toLsp) {
      expect(toLSPSymbolKind(octocodeKind)).toBe(lspKind);
    }

    expect(convertSymbolKind(999 as SymbolKind)).toBe('unknown');
    expect(toLSPSymbolKind('unknown')).toBe(SymbolKind.Function);
  });

  it('validates config schema and server paths', async () => {
    const dir = await tempDir('octocode-lsp-validation-');
    const bin = path.join(dir, 'server.js');
    await writeFile(bin, '#!/usr/bin/env node\n');

    expect(
      LSPConfigFileSchema.safeParse({
        languageServers: {
          '.foo': {
            command: bin,
            args: ['--stdio'],
            languageId: 'foo',
            initializationOptions: { strict: true },
          },
        },
      }).success
    ).toBe(true);
    expect(
      LSPConfigFileSchema.safeParse({
        languageServers: { foo: { command: bin, languageId: 'foo' } },
      }).success
    ).toBe(false);

    expect(validateLSPServerPath(bin, dir)).toMatchObject({
      isValid: true,
      resolvedPath: bin,
    });
    expect(validateLSPServerPath('../outside', dir).isValid).toBe(false);
    expect(validateLSPServerPath(path.join(dir, 'missing'), dir)).toMatchObject(
      {
        isValid: false,
        error: 'LSP server binary not found',
      }
    );
    expect(validateLSPServerPath(dir, dir)).toMatchObject({
      isValid: false,
      error: 'LSP server path is not a file',
    });

    const loop = path.join(dir, 'loop');
    await symlink(loop, loop);
    expect(validateLSPServerPath(loop, dir)).toMatchObject({
      isValid: false,
      error: 'Symlink loop detected in LSP server path',
    });
  });

  it('safeReadFile respects path validation and read failures', async () => {
    const dir = await tempDir('octocode-lsp-read-');
    const file = path.join(dir, 'file.ts');
    await writeFile(file, 'export const value = 1;\n');

    expect(await safeReadFile(file)).toContain('value');
    expect(await safeReadFile(path.join(dir, 'missing.ts'))).toBeNull();
    expect(await safeReadFile('/etc/octocode-lsp-secret.ts')).toBeNull();
  });

  it('resolves symbols by exact line, nearby line, whole-file scan, and errors', () => {
    const resolver = new SymbolResolver({ lineSearchRadius: 2 });
    const content = [
      'const text = "target";',
      '// target in comment',
      'function target() {}',
      'const other = target();',
      'const targetAgain = 1;',
    ].join('\n');

    expect(
      resolver.resolvePositionFromContent(content, {
        symbolName: 'target',
        lineHint: 3,
      })
    ).toMatchObject({ foundAtLine: 3, position: { line: 2, character: 9 } });
    expect(
      resolver.resolvePositionFromContent(content, {
        symbolName: 'target',
        lineHint: 5,
      })
    ).toMatchObject({ foundAtLine: 4, lineOffset: -1 });
    expect(
      resolver.resolvePositionFromContent(content, { symbolName: 'target' })
    ).toMatchObject({ foundAtLine: 3 });
    expect(() =>
      resolver.resolvePositionFromContent(content, {
        symbolName: 'missing',
        lineHint: 999,
      })
    ).toThrow(SymbolResolutionError);
    expect(() =>
      resolver.resolvePositionFromContent(content, {
        symbolName: 'missing',
        lineHint: 3,
      })
    ).toThrow(/within ±2 lines/);
    expect(resolver.extractContext(content, 3, 1)).toEqual({
      content: [
        '// target in comment',
        'function target() {}',
        'const other = target();',
      ].join('\n'),
      startLine: 2,
      endLine: 4,
    });
  });

  it('resolves symbol edge cases from files and content', async () => {
    const dir = await tempDir('octocode-lsp-symbol-file-');
    const file = path.join(dir, 'symbols.ts');
    const content = [
      'const value = "target";',
      '// target in a comment',
      'const xéx = 1;',
      'const x = 2;',
      'const templated = `ok ${target}`;',
      'const nested = `ok ${{ target }}`;',
      'target(target);',
    ].join('\n');
    await writeFile(file, content);
    const resolver = new SymbolResolver({ lineSearchRadius: 1 });

    await expect(resolveSymbolPosition(file, 'target')).resolves.toMatchObject({
      foundAtLine: 5,
      position: { line: 4, character: 24 },
    });
    expect(
      resolver.resolvePositionFromContent(content, {
        symbolName: 'target',
        lineHint: 7,
        orderHint: 1,
      })
    ).toMatchObject({ foundAtLine: 7, position: { line: 6, character: 7 } });
    expect(
      resolver.resolvePositionFromContent(content, {
        symbolName: 'target',
        lineHint: 6,
      })
    ).toMatchObject({ foundAtLine: 6 });
    expect(
      resolver.resolvePositionFromContent(content, { symbolName: 'x' })
    ).toMatchObject({ foundAtLine: 4, position: { line: 3, character: 6 } });
    expect(() =>
      resolver.resolvePositionFromContent(content, { symbolName: 'missing' })
    ).toThrow(SymbolResolutionError);
  });

  it('attaches LSP evidence for success, empty, and paginated results', () => {
    expect(
      attachLspEvidence(
        { data: [1] },
        { kind: 'references', paginationKey: 'pagination' }
      )
    ).toMatchObject({
      evidence: { answerReady: true, complete: true, confidence: 'high' },
    });
    expect(
      attachLspEvidence(
        { status: 'empty' },
        { kind: 'references', paginationKey: 'pagination' }
      )
    ).toMatchObject({
      evidence: {
        answerReady: false,
        complete: false,
        reason: expect.stringContaining('No references'),
      },
    });
    expect(
      attachLspEvidence(
        { status: 'empty' },
        { kind: 'calls', paginationKey: 'pagination' }
      )
    ).toMatchObject({
      evidence: {
        reason: expect.stringContaining('No calls'),
      },
    });
    expect(
      attachLspEvidence(
        { pagination: { hasMore: true } },
        { kind: 'references', paginationKey: 'pagination' }
      )
    ).toMatchObject({
      evidence: {
        complete: false,
        reason: expect.stringContaining('result pagination'),
      },
    });
    expect(
      attachLspEvidence(
        { outputPagination: { hasMore: true } },
        { kind: 'references', paginationKey: 'outputPagination' }
      )
    ).toMatchObject({
      evidence: {
        reason: expect.stringContaining('output pagination'),
      },
    });
    expect(
      attachLspEvidence(
        { status: 'error' },
        { kind: 'references', paginationKey: 'pagination' }
      )
    ).toEqual({ status: 'error' });
  });

  it('finds workspace roots from markers and configured cwd', async () => {
    const root = await tempDir('octocode-lsp-root-');
    const nested = path.join(root, 'packages', 'pkg', 'src');
    await mkdir(nested, { recursive: true });
    await writeFile(path.join(root, 'package.json'), '{}');
    const file = path.join(nested, 'index.ts');
    await writeFile(file, '');

    expect(await findWorkspaceRoot(file)).toBe(root);

    const oldCwd = process.cwd();
    process.chdir(path.join(root, 'packages', 'pkg'));
    try {
      expect(await resolveWorkspaceRootForFile(file)).toBe(process.cwd());
    } finally {
      process.chdir(oldCwd);
    }

    const markerless = path.join(
      await tempDir('octocode-lsp-rootless-'),
      'a',
      'b'
    );
    await mkdir(markerless, { recursive: true });
    const markerlessFile = path.join(markerless, 'x.ts');
    await writeFile(markerlessFile, '');
    expect(await findWorkspaceRoot(markerlessFile)).toBe(markerless);
    expect(existsSync(markerlessFile)).toBe(true);
  });
});
