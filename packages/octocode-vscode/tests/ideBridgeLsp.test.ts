import * as fs from 'fs/promises';
import * as os from 'os';
import * as path from 'path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('vscode', async () => (await import('./ideBridgeMock')).vscodeMock());

import {
  at,
  ctx,
  FILE,
  lsp,
  range,
  useBridge,
  vsUri,
} from './ideBridgeHarness';
import { host } from './ideBridgeMock';

useBridge();

describe('request validation', () => {
  beforeEach(() => ctx.bridge.start());

  it.each([
    ['no method', {}],
    ['a numeric method', { method: 1 }],
    ['a relative path', at('definition', { path: 'src/main.ts' })],
    ['no path', { method: 'documentSymbols' }],
    ['no line', { method: 'hover', path: FILE, character: 1 }],
    ['a negative line', at('hover', { line: -1 })],
    ['a fractional character', at('hover', { character: 1.5 })],
    ['a string line', at('hover', { line: '3' })],
    ['no workspace query', { method: 'workspaceSymbol' }],
    [
      'a relative workspace path',
      { method: 'workspaceSymbol', query: 'a', path: 'x' },
    ],
    ['no item', { method: 'incomingCalls' }],
    [
      'an item without a handle',
      { method: 'incomingCalls', item: { name: 'a' } },
    ],
    ['a non-object item', { method: 'subtypes', item: 'x' }],
  ])('400 for %s', async (_name, body) => {
    const reply = await lsp(body);
    expect(reply.status).toBe(400);
    expect(host.execute).not.toHaveBeenCalled();
  });

  it('404 for an unknown method, naming the supported ones', async () => {
    for (const method of ['rename', '__proto__', 'constructor', 'toString']) {
      const reply = await lsp(at(method));
      expect(reply.status).toBe(404);
      expect(reply.json.reason).toContain('definition');
    }
    expect(host.execute).not.toHaveBeenCalled();
  });

  it('422 when the document cannot be opened', async () => {
    host.open.mockRejectedValue(new Error('file not found'));
    const reply = await lsp(at('definition'));
    expect(reply.status).toBe(422);
    expect(reply.json).toEqual({
      error: 'cannot open document',
      reason: 'file not found',
    });
  });

  it('500 when a provider command fails or returns malformed data', async () => {
    host.execute.mockRejectedValue(new Error('provider crashed'));
    expect(await lsp(at('references'))).toMatchObject({
      status: 500,
      json: { error: 'provider failed', reason: 'provider crashed' },
    });
    host.diagnostics.mockImplementation(() => {
      throw new Error('no diagnostics');
    });
    expect((await lsp({ method: 'diagnostics', path: FILE })).status).toBe(500);
    host.execute.mockReset().mockResolvedValue([{ uri: vsUri() }]);
    const malformed = await lsp(at('definition'));
    expect(malformed.status).toBe(500);
    expect(malformed.json.error).toBe('bridge failure');
    expect(ctx.log.join('\n')).toContain('request failed');
  });
});

describe('method mapping', () => {
  beforeEach(() => ctx.bridge.start());

  it.each([
    ['definition', 'vscode.executeDefinitionProvider'],
    ['typeDefinition', 'vscode.executeTypeDefinitionProvider'],
    ['implementation', 'vscode.executeImplementationProvider'],
    ['references', 'vscode.executeReferenceProvider'],
    ['hover', 'vscode.executeHoverProvider'],
    ['prepareCallHierarchy', 'vscode.prepareCallHierarchy'],
    ['prepareTypeHierarchy', 'vscode.prepareTypeHierarchy'],
  ])(
    '%s calls %s with a Uri and a 0-based Position',
    async (method, command) => {
      host.execute.mockResolvedValue([
        {
          uri: vsUri(),
          range: range(),
          name: 'x',
          kind: 0,
          selectionRange: range(),
        },
      ]);
      const reply = await lsp(at(method));
      expect(reply.status).toBe(200);
      expect(host.open).toHaveBeenCalledWith(
        expect.objectContaining({ fsPath: FILE })
      );
      const [name, uri, position] = host.execute.mock.calls[0];
      expect(name).toBe(command);
      expect(uri).toMatchObject({ fsPath: FILE });
      expect(position).toMatchObject({ line: 3, character: 7 });
      expect(position.constructor.name).toBe('Position');
    }
  );

  it.each([['documentSymbols', 'vscode.executeDocumentSymbolProvider']])(
    '%s calls %s with the Uri only',
    async (method, command) => {
      host.execute.mockResolvedValue([
        {
          name: 'a',
          kind: 11,
          detail: '',
          range: range(),
          selectionRange: range(),
        },
      ]);
      await lsp({ method, path: FILE });
      expect(host.execute).toHaveBeenCalledTimes(1);
      expect(host.execute.mock.calls[0]).toEqual([
        command,
        expect.objectContaining({ fsPath: FILE }),
      ]);
    }
  );
});

describe('workspace symbols and diagnostics', () => {
  beforeEach(() => ctx.bridge.start());

  it('workspaceSymbol passes the query, opening a document only when a path is given', async () => {
    host.execute.mockResolvedValue([
      {
        name: 'Thing',
        kind: 4,
        containerName: 'pkg',
        location: { uri: vsUri(), range: range() },
      },
    ]);
    const reply = await lsp({ method: 'workspaceSymbol', query: 'Thing' });
    expect(host.execute).toHaveBeenCalledWith(
      'vscode.executeWorkspaceSymbolProvider',
      'Thing'
    );
    expect(host.open).not.toHaveBeenCalled();
    expect(reply.json).toEqual({
      result: [
        {
          name: 'Thing',
          kind: 5,
          containerName: 'pkg',
          location: { uri: 'file:///work/src/other.ts', range: range() },
        },
      ],
    });
    host.execute.mockResolvedValue([]);
    expect((await lsp({ method: 'workspaceSymbol', query: '' })).json).toEqual({
      result: [],
    });
    await lsp({ method: 'workspaceSymbol', query: 'T', path: FILE });
    expect(host.open).toHaveBeenCalled();
  });

  it('diagnostics reads vscode.languages.getDiagnostics(uri)', async () => {
    host.diagnostics.mockReturnValue([
      {
        range: range(0, 0, 0, 3),
        message: 'bad',
        severity: 0,
        source: 'ts',
        code: { value: 2304, target: vsUri() },
        tags: [1],
        relatedInformation: [
          {
            location: { uri: vsUri(), range: range() },
            message: 'declared here',
          },
        ],
      },
      { range: range(), message: 'hint', severity: 3, code: 'E1' },
      { range: range(), message: 'plain', severity: 1, code: 7 },
    ]);
    const reply = await lsp({ method: 'diagnostics', path: FILE });
    expect(host.diagnostics).toHaveBeenCalledWith(
      expect.objectContaining({ fsPath: FILE })
    );
    expect(host.execute).not.toHaveBeenCalled();
    expect(reply.json.result).toEqual([
      {
        range: range(0, 0, 0, 3),
        severity: 1,
        message: 'bad',
        code: 2304,
        source: 'ts',
        tags: [1],
        relatedInformation: [
          {
            location: { uri: 'file:///work/src/other.ts', range: range() },
            message: 'declared here',
          },
        ],
      },
      { range: range(), severity: 4, message: 'hint', code: 'E1' },
      { range: range(), severity: 2, message: 'plain', code: 7 },
    ]);
  });
});

describe('workspace confinement', () => {
  beforeEach(() => ctx.bridge.start());

  it.each([
    ['position', at('definition', { path: '/etc/passwd' })],
    ['document', { method: 'documentSymbols', path: '/etc/passwd' }],
    [
      'workspace',
      { method: 'workspaceSymbol', query: 'a', path: '/etc/passwd' },
    ],
    ['a sibling prefix', at('hover', { path: '/work-other/a.ts' })],
  ])('403 for a %s path outside every workspace folder', async (_n, body) => {
    const reply = await lsp(body);
    expect(reply.status).toBe(403);
    expect(reply.json.error).toEqual(expect.any(String));
    expect(reply.json.reason).toContain(path.basename(body.path));
    expect(host.open).not.toHaveBeenCalled();
    expect(host.execute).not.toHaveBeenCalled();
  });

  it('serves a path inside a workspace folder', async () => {
    host.execute.mockResolvedValue([]);
    host.folders = [
      { uri: { scheme: 'file', fsPath: '/other' } },
      ...host.folders,
    ];
    expect((await lsp(at('definition'))).status).toBe(200);
  });
});

describe.skipIf(process.platform === 'win32')(
  'symlinked workspace folder',
  () => {
    let root = '';
    let real = '';
    let link = '';
    let outside = '';

    beforeEach(async () => {
      await ctx.bridge.start();
      root = await fs.realpath(
        await fs.mkdtemp(path.join(os.tmpdir(), 'octocode-symlink-'))
      );
      real = path.join(root, 'real');
      link = path.join(root, 'link');
      outside = path.join(root, 'outside');
      await fs.mkdir(path.join(real, 'src'), { recursive: true });
      await fs.mkdir(outside);
      await fs.writeFile(path.join(real, 'src', 'a.ts'), '');
      await fs.writeFile(path.join(outside, 'b.ts'), '');
      await fs.symlink(real, link);
      host.folders = [{ uri: { scheme: 'file', fsPath: link } }];
      host.execute.mockResolvedValue([]);
    });

    afterEach(() => fs.rm(root, { recursive: true, force: true }));

    it('serves a canonical path and opens the folder spelling', async () => {
      const reply = await lsp(
        at('definition', { path: path.join(real, 'src', 'a.ts') })
      );
      expect(reply.status).toBe(200);
      expect(host.open).toHaveBeenCalledWith(
        expect.objectContaining({ fsPath: path.join(link, 'src', 'a.ts') })
      );
    });

    it('still 403s a canonical path outside the real folder', async () => {
      const reply = await lsp(
        at('definition', { path: path.join(outside, 'b.ts') })
      );
      expect(reply.status).toBe(403);
      expect(host.open).not.toHaveBeenCalled();
    });

    it('403s a symlink inside the folder that leads outside', async () => {
      await fs.symlink(outside, path.join(real, 'escape'));
      const reply = await lsp(
        at('definition', { path: path.join(link, 'escape', 'b.ts') })
      );
      expect(reply.status).toBe(403);
    });
  }
);

describe('workspaceSymbol scope', () => {
  beforeEach(() => ctx.bridge.start());

  it('does not open a document for a directory path', async () => {
    const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'octocode-ws-'));
    try {
      host.folders = [{ uri: { scheme: 'file', fsPath: dir } }];
      host.execute.mockResolvedValue([]);
      const reply = await lsp({
        method: 'workspaceSymbol',
        query: 'T',
        path: dir,
      });
      expect(reply.json).toEqual({ result: [] });
      expect(host.open).not.toHaveBeenCalled();
      expect(host.execute).toHaveBeenCalledWith(
        'vscode.executeWorkspaceSymbolProvider',
        'T'
      );
    } finally {
      await fs.rm(dir, { recursive: true, force: true });
    }
  });
});
