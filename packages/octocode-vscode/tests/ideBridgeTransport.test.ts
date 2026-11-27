import { beforeEach, describe, expect, it } from 'vitest';

vi.mock('vscode', async () => (await import('./ideBridgeMock')).vscodeMock());

import { MAX_BODY_BYTES } from '../src/ideBridgeHttp';
import { IDE_BRIDGE_METHODS } from '../src/ideBridgeMethods';
import {
  at,
  authed,
  ctx,
  lock,
  lsp,
  range,
  useBridge,
  vsUri,
} from './ideBridgeHarness';
import { host } from './ideBridgeMock';

useBridge();

describe('health and auth', () => {
  beforeEach(() => ctx.bridge.start());

  it('answers /health with version, ide, folders and methods', async () => {
    const reply = await authed({});
    expect(reply.status).toBe(200);
    expect(reply.headers['content-type']).toContain('application/json');
    expect(reply.json).toEqual({
      version: 1,
      ide: 'Test IDE',
      workspaceFolders: ['/work'],
      methods: IDE_BRIDGE_METHODS,
    });
    expect(reply.json.methods).toEqual(
      expect.arrayContaining([
        'definition',
        'typeDefinition',
        'implementation',
        'references',
        'hover',
        'documentSymbols',
        'workspaceSymbol',
        'prepareCallHierarchy',
        'incomingCalls',
        'outgoingCalls',
        'prepareTypeHierarchy',
        'supertypes',
        'subtypes',
        'diagnostics',
      ])
    );
  });

  it.each([
    ['no Authorization', { Authorization: undefined }],
    ['a wrong token', { Authorization: `Bearer ${'0'.repeat(64)}` }],
    ['a short token', { Authorization: 'Bearer x' }],
    ['an empty token', { Authorization: 'Bearer ' }],
    ['another scheme', { Authorization: 'Basic abc' }],
  ])('401 with %s, without echoing the token', async (_name, headers) => {
    const { token } = await lock();
    for (const request of [
      authed({}, headers),
      lsp(at('definition'), headers),
    ]) {
      const reply = await request;
      expect(reply.status).toBe(401);
      expect(reply.text).not.toContain(token);
      expect(JSON.stringify(reply.headers)).not.toContain(token);
      expect(reply.json.error).toEqual(expect.any(String));
    }
    expect(host.execute).not.toHaveBeenCalled();
  });
});

describe('host header and routes', () => {
  beforeEach(() => ctx.bridge.start());

  it.each([
    ['a different port', (port: number) => `127.0.0.1:${port + 1}`],
    ['a hostname', (port: number) => `localhost:${port}`],
    ['no port', () => '127.0.0.1'],
    ['a rebinding name', (port: number) => `evil.example:${port}`],
  ])('400 on %s as Host', async (_name, host_) => {
    const { port } = await lock();
    const reply = await authed({}, { Host: host_(port) });
    expect(reply.status).toBe(400);
    expect((await lsp(at('definition'), { Host: host_(port) })).status).toBe(
      400
    );
    expect(host.execute).not.toHaveBeenCalled();
  });

  it('404 for an unknown route and 405 for the wrong verb', async () => {
    expect((await authed({ url: '/nope' })).status).toBe(404);
    expect(
      (await authed({ url: '/health', method: 'POST' })).headers.allow
    ).toBe('GET');
    expect((await authed({ url: '/lsp' })).status).toBe(405);
    const put = await authed({ url: '/lsp', method: 'PUT' });
    expect(put.status).toBe(405);
    expect(put.headers.allow).toBe('POST');
  });
});

describe('request bodies', () => {
  beforeEach(() => ctx.bridge.start());

  it('400 for a non-JSON content type, bad JSON and a non-object body', async () => {
    expect(
      (
        await authed({
          method: 'POST',
          url: '/lsp',
          headers: { 'Content-Type': 'text/plain' },
          body: '{}',
        })
      ).status
    ).toBe(400);
    expect(
      (await authed({ method: 'POST', url: '/lsp', body: '{}' })).status
    ).toBe(400);
    expect((await lsp('{ nope')).status).toBe(400);
    expect((await lsp('[1]')).status).toBe(400);
    expect((await lsp('null')).status).toBe(400);
    const charset = await authed({
      method: 'POST',
      url: '/lsp',
      headers: { 'Content-Type': 'application/json; charset=utf-8' },
      body: JSON.stringify({ method: 'nope' }),
    });
    expect(charset.status).toBe(404);
  });

  it('413 when Content-Length exceeds 1 MiB, without running a provider', async () => {
    const body = JSON.stringify({
      ...at('definition'),
      pad: 'x'.repeat(MAX_BODY_BYTES),
    });
    const reply = await lsp(body);
    expect(reply.status).toBe(413);
    expect(host.execute).not.toHaveBeenCalled();
  });

  it('413 for a chunked body that grows past the limit', async () => {
    const chunk = Buffer.alloc(256 * 1024, 'x');
    const reply = await authed({
      method: 'POST',
      url: '/lsp',
      headers: { 'Content-Type': 'application/json' },
      chunks: Array.from({ length: 6 }, () => chunk),
    });
    expect(reply.status).toBe(413);
    expect(reply.json.reason).toContain(String(MAX_BODY_BYTES));
  });

  it('accepts a body just under the limit', async () => {
    host.execute.mockResolvedValue([]);
    host.execute.mockResolvedValueOnce([{ uri: vsUri(), range: range() }]);
    const reply = await lsp({
      ...at('definition'),
      pad: 'x'.repeat(MAX_BODY_BYTES - 512),
    });
    expect(reply.status).toBe(200);
  });
});
