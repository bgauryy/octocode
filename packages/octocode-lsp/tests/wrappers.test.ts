import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';

import { describe, expect, it, vi } from 'vitest';

function nativeMock() {
  const client = {
    start: vi.fn().mockResolvedValue(undefined),
    stop: vi.fn().mockResolvedValue(undefined),
    waitForReady: vi.fn().mockResolvedValue(undefined),
    getRecentStderr: vi.fn((): string[] => []),
    openDocument: vi.fn().mockResolvedValue(undefined),
    getDefinition: vi.fn().mockResolvedValue([{ uri: 'file:///a.ts' }]),
    getReferences: vi.fn().mockResolvedValue([{ uri: 'file:///b.ts' }]),
    getHover: vi.fn().mockResolvedValue({ contents: 'hover' }),
    getTypeDefinition: vi.fn().mockResolvedValue([{ uri: 'file:///c.ts' }]),
    getImplementation: vi.fn().mockResolvedValue([{ uri: 'file:///d.ts' }]),
    getDocumentSymbols: vi.fn().mockResolvedValue([{ name: 'symbol' }]),
    prepareCallHierarchy: vi.fn().mockResolvedValue([{ name: 'call' }]),
    incomingCalls: vi.fn().mockResolvedValue([{ from: { name: 'caller' } }]),
    outgoingCalls: vi.fn().mockResolvedValue([{ to: { name: 'callee' } }]),
  };
  const NativeLspClient = vi.fn(function NativeLspClient() {
    return client;
  });
  return {
    client,
    nativeBinding: {
      NativeLspClient,
      resolvePosition: vi.fn(() => ({
        position: { line: 1, character: 2 },
        foundAtLine: 2,
        lineOffset: 0,
        lineContent: 'function target() {}',
      })),
      resolvePositionFromContent: vi.fn(() => ({
        position: { line: 1, character: 2 },
        found_at_line: 2,
        line_offset: 0,
        line_content: 'function target() {}',
      })),
      toUri: vi.fn((filePath: string) => `file://${filePath}`),
      fromUri: vi.fn((uri: string) => uri.replace(/^file:\/\//, '')),
      resolveWorkspaceRootForFile: vi.fn(() => '/workspace'),
      detectLanguageId: vi.fn((filePath: string) =>
        filePath.endsWith('.ts') ? 'typescript' : undefined
      ),
      getLanguageServerForFile: vi.fn(() => ({
        command: 'typescript-language-server',
        args: ['--stdio'],
        workspaceRoot: '/workspace',
        languageId: 'typescript',
      })),
      isCommandAvailable: vi.fn(() => true),
      safeReadFile: vi.fn(() => 'content'),
      validateLspServerPath: vi.fn((command: string) => command),
    },
  };
}

async function withMockedNative<T>(
  run: (mock: ReturnType<typeof nativeMock>) => Promise<T>
) {
  vi.resetModules();
  const mock = nativeMock();
  vi.doMock('../src/native.js', () => mock);
  try {
    return await run(mock);
  } finally {
    vi.doUnmock('../src/native.js');
    vi.resetModules();
  }
}

describe('TypeScript wrappers delegate to nativeBinding only', () => {
  it('delegates every LSPClient operation to the native client', async () => {
    await withMockedNative(async mock => {
      const { LSPClient } = await import('../src/client.js');
      const root = await mkdtemp(
        path.join(os.tmpdir(), 'octocode-lsp-wrapper-')
      );
      const filePath = path.join(root, 'a.ts');
      await writeFile(filePath, 'function target() {}\n');
      const client = new LSPClient({
        command: 'server',
        args: ['--stdio'],
        workspaceRoot: root,
        languageId: 'typescript',
        initializationOptions: { strict: true },
      });
      const position = { line: 1, character: 2 };
      const item = {
        name: 'target',
        kind: 'function' as const,
        uri: `file://${filePath}`,
        range: { start: position, end: position },
      };

      await client.start();
      expect(client.hasCapability('definitionProvider')).toBe(true);
      Object.assign(mock.client, {
        hasCapability: vi.fn().mockReturnValue(false),
      });
      expect(client.hasCapability('definitionProvider')).toBe(false);
      await client.waitForReady(10);
      await expect(
        client.getDefinition(filePath, position)
      ).resolves.toHaveLength(1);
      await expect(
        client.gotoDefinition(filePath, position)
      ).resolves.toHaveLength(1);
      await expect(
        client.findReferences(filePath, position, false)
      ).resolves.toHaveLength(1);
      await expect(client.getHover(filePath, position)).resolves.toEqual({
        contents: 'hover',
      });
      await expect(client.hover(filePath, position)).resolves.toEqual({
        contents: 'hover',
      });
      await expect(
        client.getTypeDefinition(filePath, position)
      ).resolves.toHaveLength(1);
      await expect(
        client.typeDefinition(filePath, position)
      ).resolves.toHaveLength(1);
      await expect(
        client.getImplementation(filePath, position)
      ).resolves.toHaveLength(1);
      await expect(
        client.implementation(filePath, position)
      ).resolves.toHaveLength(1);
      await expect(client.getDocumentSymbols(filePath)).resolves.toEqual([
        { name: 'symbol' },
      ]);
      await expect(client.documentSymbols(filePath)).resolves.toEqual([
        { name: 'symbol' },
      ]);
      await expect(
        client.prepareCallHierarchy(filePath, position)
      ).resolves.toEqual([{ name: 'call' }]);
      mock.client.prepareCallHierarchy.mockResolvedValueOnce(null);
      await expect(
        client.prepareCallHierarchy(filePath, position)
      ).resolves.toEqual([]);
      await expect(client.getIncomingCalls(item)).resolves.toEqual([
        { from: { name: 'caller' } },
      ]);
      mock.client.incomingCalls.mockResolvedValueOnce(null);
      await expect(client.getIncomingCalls(item)).resolves.toEqual([]);
      await expect(client.getOutgoingCalls(item)).resolves.toEqual([
        { to: { name: 'callee' } },
      ]);
      mock.client.outgoingCalls.mockResolvedValueOnce(null);
      await expect(client.getOutgoingCalls(item)).resolves.toEqual([]);
      await client.ensureDocumentSynced(filePath, 'content');
      await client.openDocument(filePath, 'content');
      await expect(client.closeDocument(filePath)).resolves.toBeUndefined();
      await client.stop();
      expect(client.hasCapability('definitionProvider')).toBe(false);
      expect(client.getRecentStderr()).toEqual([]);
      expect(mock.client.getDefinition).toHaveBeenCalled();
      await rm(root, { recursive: true, force: true });
    });
  });

  it('uses supplied document content for LSP requests that open a file', async () => {
    await withMockedNative(async mock => {
      const { LSPClient } = await import('../src/client.js');
      const root = await mkdtemp(
        path.join(os.tmpdir(), 'octocode-lsp-wrapper-')
      );
      const filePath = path.join(root, 'cached.ts');
      await writeFile(filePath, 'from disk\n');
      const client = new LSPClient({
        command: 'server',
        args: ['--stdio'],
        workspaceRoot: root,
        languageId: 'typescript',
      });
      const position = { line: 0, character: 16 };
      const cachedContent = 'from cached anchor\n';

      await client.getDefinition(filePath, position, cachedContent);
      await client.gotoDefinition(filePath, position, cachedContent);
      await client.findReferences(filePath, position, false, cachedContent);
      await client.getHover(filePath, position, cachedContent);
      await client.hover(filePath, position, cachedContent);
      await client.getTypeDefinition(filePath, position, cachedContent);
      await client.typeDefinition(filePath, position, cachedContent);
      await client.getImplementation(filePath, position, cachedContent);
      await client.implementation(filePath, position, cachedContent);
      await client.getDocumentSymbols(filePath, cachedContent);
      await client.documentSymbols(filePath, cachedContent);
      await client.prepareCallHierarchy(filePath, position, cachedContent);

      expect(mock.client.openDocument.mock.calls).toEqual(
        Array.from({ length: 12 }, () => [filePath, cachedContent])
      );
      await rm(root, { recursive: true, force: true });
    });
  });

  it('surfaces recent native stderr lines', async () => {
    await withMockedNative(async mock => {
      const { LSPClient } = await import('../src/client.js');
      mock.client.getRecentStderr.mockReturnValueOnce(['server warning']);
      const client = new LSPClient({
        command: 'server',
        args: ['--stdio'],
        workspaceRoot: '/workspace',
        languageId: 'typescript',
      });

      expect(client.getRecentStderr()).toEqual(['server warning']);
    });
  });

  it('delegates resolver, uri, config, validation, and workspace helpers', async () => {
    await withMockedNative(async mock => {
      const resolverModule = await import('../src/resolver.js');
      const uriModule = await import('../src/uri.js');
      const configModule = await import('../src/config.js');
      const validationModule = await import('../src/validation.js');
      const workspaceModule = await import('../src/workspaceRoot.js');
      const filePath = '/workspace/a.ts';

      const defaultResolver = new resolverModule.SymbolResolver();
      expect(defaultResolver.lineSearchRadius).toBe(5);
      expect(
        new resolverModule.SymbolResolutionError('target', 3, 'missing', 9)
      ).toMatchObject({
        name: 'SymbolResolutionError',
        symbolName: 'target',
        lineHint: 3,
        reason: 'missing',
        searchRadius: 9,
      });
      const resolver = new resolverModule.SymbolResolver({
        lineSearchRadius: 7,
      });
      expect(resolver.lineSearchRadius).toBe(7);
      await expect(
        resolver.resolvePosition(filePath, { symbolName: 'target' })
      ).resolves.toMatchObject({ foundAtLine: 2 });
      expect(
        resolver.resolvePositionFromContent('content', { symbolName: 'target' })
      ).toMatchObject({ foundAtLine: 2 });
      await expect(
        resolverModule.resolveSymbolPosition(filePath, 'target', 2, 0)
      ).resolves.toMatchObject({ foundAtLine: 2 });
      mock.nativeBinding.resolvePosition.mockReturnValueOnce({
        position: { line: 0, character: 0 },
      } as ReturnType<typeof mock.nativeBinding.resolvePosition>);
      await expect(
        resolverModule.resolveSymbolPosition(filePath, 'target')
      ).resolves.toMatchObject({
        foundAtLine: 0,
        lineOffset: 0,
        lineContent: '',
      });
      expect(
        resolverModule.resolveSymbolPosition('content', {
          symbolName: 'target',
        })
      ).toMatchObject({ foundAtLine: 2 });
      mock.nativeBinding.resolvePosition.mockImplementationOnce(() => {
        throw new resolverModule.SymbolResolutionError(
          'target',
          3,
          'already normalized',
          11
        );
      });
      await expect(
        resolver.resolvePosition(filePath, {
          symbolName: 'target',
          lineHint: 3,
        })
      ).rejects.toMatchObject({
        reason: 'already normalized',
        searchRadius: 11,
      });
      mock.nativeBinding.resolvePosition.mockImplementationOnce(() => {
        throw new Error('native missing');
      });
      await expect(
        resolver.resolvePosition(filePath, { symbolName: 'target' })
      ).rejects.toMatchObject({
        symbolName: 'target',
        lineHint: 0,
        reason: 'native missing',
        searchRadius: 7,
      });
      mock.nativeBinding.resolvePositionFromContent.mockImplementationOnce(
        () => {
          throw 'native string';
        }
      );
      let thrown: unknown;
      try {
        resolver.resolvePositionFromContent('content', {
          symbolName: 'target',
          lineHint: 6,
        });
      } catch (error) {
        thrown = error;
      }
      expect(thrown).toBeInstanceOf(resolverModule.SymbolResolutionError);
      expect(thrown).toMatchObject({
        symbolName: 'target',
        lineHint: 6,
        reason: 'native string',
        searchRadius: 7,
      });

      expect(uriModule.toUri(filePath)).toBe('file:///workspace/a.ts');
      expect(uriModule.fromUri('file:///workspace/a.ts')).toBe(filePath);
      expect(uriModule.fromUriSafe('file:///workspace/a.ts')).toEqual({
        isValid: true,
        path: filePath,
      });
      mock.nativeBinding.fromUri.mockImplementationOnce(() => {
        throw new Error('bad uri');
      });
      expect(uriModule.fromUriSafe('bad')).toEqual({
        isValid: false,
        error: 'bad uri',
      });
      mock.nativeBinding.fromUri.mockImplementationOnce(() => {
        throw 'bad string';
      });
      expect(uriModule.fromUriSafe('bad')).toEqual({
        isValid: false,
        error: 'bad string',
      });
      mock.nativeBinding.fromUri.mockImplementationOnce(() => {
        throw new Error('bad uri');
      });
      expect(() =>
        uriModule.fromUriSafe('bad', { throwOnInvalid: true })
      ).toThrow(uriModule.UnsafeUriError);

      expect(configModule.detectLanguageId(filePath)).toBe('typescript');
      expect(configModule.detectLanguageId('/workspace/a.unknown')).toBe(
        'plaintext'
      );
      await expect(
        configModule.getLanguageServerForFile(filePath)
      ).resolves.toMatchObject({ languageId: 'typescript' });
      await expect(validationModule.safeReadFile(filePath)).resolves.toBe(
        'content'
      );
      mock.nativeBinding.safeReadFile.mockImplementationOnce(() => {
        throw new Error('missing');
      });
      await expect(
        validationModule.safeReadFile('/workspace/missing.ts')
      ).resolves.toBeNull();
      expect(validationModule.validateLSPServerPath('server')).toEqual({
        isValid: true,
        resolvedPath: 'server',
      });
      mock.nativeBinding.validateLspServerPath.mockImplementationOnce(() => {
        throw new Error('bad command');
      });
      expect(validationModule.validateLSPServerPath('bad')).toEqual({
        isValid: false,
        error: 'bad command',
      });
      mock.nativeBinding.validateLspServerPath.mockImplementationOnce(() => {
        throw 'bad string';
      });
      expect(validationModule.validateLSPServerPath('bad')).toEqual({
        isValid: false,
        error: 'bad string',
      });
      await expect(
        workspaceModule.resolveWorkspaceRootForFile(filePath)
      ).resolves.toBe('/workspace');
      await expect(workspaceModule.findWorkspaceRoot(filePath)).resolves.toBe(
        '/workspace'
      );
    });
  });
});
