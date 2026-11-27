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

describe('unsupported answers', () => {
  beforeEach(() => ctx.bridge.start());

  const empty = (languageId: string) =>
    host.open.mockImplementation(async (uri: unknown) => ({ uri, languageId }));

  it('plaintext and unregistered languages answer unsupported when empty', async () => {
    host.execute.mockResolvedValue([]);
    empty('plaintext');
    expect((await lsp(at('definition'))).json).toEqual({ unsupported: true });
    empty('mystery');
    expect((await lsp(at('references'))).json).toEqual({ unsupported: true });
  });

  it('an empty answer with no document symbols is unsupported', async () => {
    host.execute.mockResolvedValue([]);
    expect((await lsp(at('definition'))).json).toEqual({ unsupported: true });
    expect(host.execute).toHaveBeenLastCalledWith(
      'vscode.executeDocumentSymbolProvider',
      expect.objectContaining({ fsPath: FILE })
    );
  });

  it('a failing symbol probe is unsupported', async () => {
    host.execute
      .mockResolvedValueOnce([])
      .mockRejectedValueOnce(new Error('no provider'));
    expect((await lsp(at('implementation'))).json).toEqual({
      unsupported: true,
    });
  });

  it('an empty documentSymbols answer is unsupported without a second probe', async () => {
    host.execute.mockResolvedValue([]);
    expect((await lsp({ method: 'documentSymbols', path: FILE })).json).toEqual(
      {
        unsupported: true,
      }
    );
    expect(host.execute).toHaveBeenCalledTimes(1);
  });

  it('empty diagnostics are unsupported without symbols, real with them', async () => {
    host.execute.mockResolvedValueOnce([]);
    expect((await lsp({ method: 'diagnostics', path: FILE })).json).toEqual({
      unsupported: true,
    });
    host.execute.mockResolvedValueOnce([{ name: 'f' }]);
    expect((await lsp({ method: 'diagnostics', path: FILE })).json).toEqual({
      result: [],
    });
  });

  it('an empty answer next to live document symbols is a real empty result', async () => {
    host.execute
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([{ name: 'f' }]);
    expect((await lsp(at('references'))).json).toEqual({ result: [] });
  });

  it('a non-array symbol probe answer is unsupported', async () => {
    host.execute
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce(undefined);
    expect((await lsp(at('definition'))).json).toEqual({ unsupported: true });
  });

  it('never reports unsupported when the answer has content', async () => {
    empty('plaintext');
    host.execute.mockResolvedValue([{ uri: vsUri(), range: range() }]);
    expect((await lsp(at('definition'))).json.unsupported).toBeUndefined();
  });
});
