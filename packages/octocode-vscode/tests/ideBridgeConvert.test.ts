import { beforeEach, describe, expect, it } from 'vitest';

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

describe('LSP conversion of locations and symbols', () => {
  beforeEach(() => ctx.bridge.start());

  it('Location and LocationLink keep 0-based ranges', async () => {
    host.execute.mockResolvedValue([
      { uri: vsUri(), range: range(4, 0, 4, 9) },
      {
        originSelectionRange: range(3, 7, 3, 12),
        targetUri: vsUri('/work/a.ts'),
        targetRange: range(10, 0, 20, 1),
        targetSelectionRange: range(10, 4, 10, 8),
      },
      { targetUri: vsUri('/work/b.ts'), targetRange: range(1, 1, 2, 2) },
    ]);
    const reply = await lsp(at('definition'));
    expect(reply.json).toEqual({
      result: [
        { uri: 'file:///work/src/other.ts', range: range(4, 0, 4, 9) },
        {
          originSelectionRange: range(3, 7, 3, 12),
          targetUri: 'file:///work/a.ts',
          targetRange: range(10, 0, 20, 1),
          targetSelectionRange: range(10, 4, 10, 8),
        },
        {
          targetUri: 'file:///work/b.ts',
          targetRange: range(1, 1, 2, 2),
          targetSelectionRange: range(1, 1, 2, 2),
        },
      ],
    });
  });

  it('DocumentSymbol keeps its tree and adds 1 to the kind', async () => {
    host.execute.mockResolvedValue([
      {
        name: 'Outer',
        detail: 'class',
        kind: 4,
        tags: [1],
        range: range(0, 0, 9, 1),
        selectionRange: range(0, 6, 0, 11),
        children: [
          {
            name: 'inner',
            kind: 5,
            range: range(1, 2, 3, 3),
            selectionRange: range(1, 2, 1, 7),
            children: [],
          },
        ],
      },
    ]);
    const reply = await lsp({ method: 'documentSymbols', path: FILE });
    expect(reply.json.result).toEqual([
      {
        name: 'Outer',
        kind: 5,
        tags: [1],
        detail: 'class',
        range: range(0, 0, 9, 1),
        selectionRange: range(0, 6, 0, 11),
        children: [
          {
            name: 'inner',
            kind: 6,
            detail: '',
            range: range(1, 2, 3, 3),
            selectionRange: range(1, 2, 1, 7),
            children: [],
          },
        ],
      },
    ]);
  });
});

describe('LSP conversion of symbol information and hover', () => {
  beforeEach(() => ctx.bridge.start());

  it('SymbolInformation answers as LSP SymbolInformation', async () => {
    host.execute.mockResolvedValue([
      { name: 'f', kind: 11, location: { uri: vsUri(), range: range() } },
    ]);
    const reply = await lsp({ method: 'documentSymbols', path: FILE });
    expect(reply.json.result).toEqual([
      {
        name: 'f',
        kind: 12,
        location: { uri: 'file:///work/src/other.ts', range: range() },
      },
    ]);
  });

  it('merges hover contents into one markdown string with the first range', async () => {
    host.execute.mockResolvedValue([
      { contents: [{ value: '**sig**' }, 'plain'], range: range(3, 7, 3, 9) },
      { contents: [{ language: 'ts', value: 'const a = 1' }, { value: '' }] },
      { contents: { value: 'single' } },
      { contents: [42] },
    ]);
    const reply = await lsp(at('hover'));
    expect(reply.json).toEqual({
      result: {
        contents: '**sig**\n\nplain\n\n```ts\nconst a = 1\n```\n\nsingle',
        range: range(3, 7, 3, 9),
      },
    });
  });

  it('hover without a range omits it', async () => {
    host.execute.mockResolvedValue([{ contents: ['text'] }]);
    expect((await lsp(at('hover'))).json).toEqual({
      result: { contents: 'text' },
    });
  });

  it('an empty hover is null when a language service exists', async () => {
    host.execute
      .mockResolvedValueOnce([{ contents: [] }])
      .mockResolvedValueOnce([{ name: 'x' }]);
    expect((await lsp(at('hover'))).json).toEqual({ result: null });
  });
});
