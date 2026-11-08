import { beforeEach, describe, expect, it } from 'vitest';

vi.mock('vscode', async () => (await import('./ideBridgeMock')).vscodeMock());

import { MAX_ITEM_HANDLES } from '../src/ideBridge';
import {
  at,
  ctx,
  lock,
  lsp,
  range,
  useBridge,
  vsUri,
} from './ideBridgeHarness';
import { host } from './ideBridgeMock';

const callItem = (name: string) => ({
  name,
  kind: 11,
  tags: [1],
  detail: 'mod',
  uri: vsUri(),
  range: range(),
  selectionRange: range(1, 3, 1, 6),
});

useBridge();

describe('call hierarchy round trip', () => {
  beforeEach(() => ctx.bridge.start());

  it('prepare hands out an opaque data handle that incoming/outgoing accept', async () => {
    host.execute.mockResolvedValueOnce([callItem('run')]);
    const prepared = await lsp(at('prepareCallHierarchy'));
    const [item] = prepared.json.result;
    expect(item).toEqual({
      name: 'run',
      kind: 12,
      tags: [1],
      detail: 'mod',
      uri: 'file:///work/src/other.ts',
      range: range(),
      selectionRange: range(1, 3, 1, 6),
      data: expect.any(String),
    });
    expect(JSON.stringify(item.data)).not.toContain('/work');
    expect(host.execute).toHaveBeenCalledTimes(1);

    const caller = callItem('caller');
    host.execute.mockResolvedValueOnce([
      { from: caller, fromRanges: [range(5, 1, 5, 4)] },
    ]);
    const incoming = await lsp({ method: 'incomingCalls', item });
    expect(host.execute).toHaveBeenLastCalledWith(
      'vscode.provideIncomingCalls',
      expect.objectContaining({ name: 'run' })
    );
    expect(incoming.json.result).toEqual([
      {
        from: expect.objectContaining({
          name: 'caller',
          data: expect.any(String),
        }),
        fromRanges: [range(5, 1, 5, 4)],
      },
    ]);
    expect(incoming.json.result[0].from.data).not.toBe(item.data);

    host.execute.mockResolvedValueOnce([
      { to: callItem('callee'), fromRanges: [range(6, 1, 6, 4)] },
    ]);
    const outgoing = await lsp({
      method: 'outgoingCalls',
      item: incoming.json.result[0].from,
    });
    expect(host.execute).toHaveBeenLastCalledWith(
      'vscode.provideOutgoingCalls',
      caller
    );
    expect(outgoing.json.result[0].to.name).toBe('callee');
    expect(outgoing.json.result[0].fromRanges).toEqual([range(6, 1, 6, 4)]);
  });
});

describe('hierarchy handles', () => {
  beforeEach(() => ctx.bridge.start());

  it('an empty call list is a real answer, not unsupported', async () => {
    host.execute.mockResolvedValueOnce([callItem('run')]);
    const [item] = (await lsp(at('prepareCallHierarchy'))).json.result;
    host.execute.mockResolvedValueOnce([]);
    expect((await lsp({ method: 'incomingCalls', item })).json).toEqual({
      result: [],
    });
  });

  it('type hierarchy round-trips through supertypes and subtypes', async () => {
    host.execute.mockResolvedValueOnce([callItem('Dog')]);
    const [dog] = (await lsp(at('prepareTypeHierarchy'))).json.result;
    host.execute.mockResolvedValueOnce([callItem('Animal')]);
    const supers = await lsp({ method: 'supertypes', item: dog });
    expect(host.execute).toHaveBeenLastCalledWith(
      'vscode.provideSupertypes',
      expect.objectContaining({ name: 'Dog' })
    );
    expect(supers.json.result[0]).toMatchObject({ name: 'Animal' });
    host.execute.mockResolvedValueOnce([callItem('Puppy')]);
    const subs = await lsp({ method: 'subtypes', item: supers.json.result[0] });
    expect(host.execute).toHaveBeenLastCalledWith(
      'vscode.provideSubtypes',
      expect.objectContaining({ name: 'Animal' })
    );
    expect(subs.json.result[0]).toMatchObject({ name: 'Puppy' });
  });

  it('404 with a reason for an unknown handle, 400 for the wrong hierarchy', async () => {
    const unknown = await lsp({
      method: 'incomingCalls',
      item: { name: 'x', data: 'never-issued' },
    });
    expect(unknown.status).toBe(404);
    expect(unknown.json.error).toBe('unknown item handle');
    expect(unknown.json.reason).toContain(String(MAX_ITEM_HANDLES));
    expect(host.execute).not.toHaveBeenCalled();

    host.execute.mockResolvedValueOnce([callItem('Dog')]);
    const [dog] = (await lsp(at('prepareTypeHierarchy'))).json.result;
    const wrong = await lsp({ method: 'incomingCalls', item: dog });
    expect(wrong.status).toBe(400);
    expect(wrong.json.reason).toContain('call');
  });
});

describe('hierarchy handle bound and failures', () => {
  beforeEach(() => ctx.bridge.start());

  it('evicts the oldest handles beyond the bound and forgets them on stop', async () => {
    host.execute.mockResolvedValueOnce(
      Array.from({ length: MAX_ITEM_HANDLES + 1 }, (_, i) => callItem(`n${i}`))
    );
    const items = (await lsp(at('prepareCallHierarchy'))).json.result;
    expect(items).toHaveLength(MAX_ITEM_HANDLES + 1);
    const first = await lsp({ method: 'outgoingCalls', item: items[0] });
    expect(first.status).toBe(404);
    host.execute.mockResolvedValueOnce([]);
    const last = await lsp({ method: 'outgoingCalls', item: items.at(-1) });
    expect(last.status).toBe(200);

    const { port, token } = await lock();
    await ctx.bridge.stop();
    await ctx.bridge.start();
    const again = await lock();
    expect(again.port === port && again.token === token).toBe(false);
    expect(
      (await lsp({ method: 'outgoingCalls', item: items.at(-1) })).status
    ).toBe(404);
  });

  it('a provider failure on an item call is a 500', async () => {
    host.execute.mockResolvedValueOnce([callItem('run')]);
    const [item] = (await lsp(at('prepareCallHierarchy'))).json.result;
    host.execute.mockRejectedValueOnce(new Error('boom'));
    expect(await lsp({ method: 'incomingCalls', item })).toMatchObject({
      status: 500,
      json: { reason: 'boom' },
    });
  });
});
