import assert from 'node:assert/strict';
import { test } from 'vitest';
import type { StreamableHTTPClientTransport } from '@modelcontextprotocol/client';
import {
  createMcpOAuthFlowWithDependencies,
  type McpOAuthDependencies,
  type StoredOAuthCredential,
} from '../src/tools/mcp-oauth.js';
import type { PiContext } from '../src/types.js';

function memoryDependencies(opened: string[] = []) {
  let stored: StoredOAuthCredential = {};
  let deleted = false;
  const dependencies: McpOAuthDependencies = {
    readCredential: async () => structuredClone(stored),
    writeCredential: async (_account, value) => { stored = structuredClone(value); },
    deleteCredential: async () => { stored = {}; deleted = true; },
    openApprovedUrl: async (target) => { opened.push(target); return { ok: true }; },
  };
  return { dependencies, stored: () => stored, deleted: () => deleted };
}

function interactiveContext(approved = true): PiContext {
  return {
    hasUI: true,
    ui: { confirm: async () => approved },
  } as unknown as PiContext;
}

test('OAuth callback validates state, finishes PKCE auth, persists tokens, and revokes local credentials', async () => {
  const opened: string[] = [];
  const memory = memoryDependencies(opened);
  const flow = await createMcpOAuthFlowWithDependencies(
    'remote',
    'https://mcp.example.test/api',
    interactiveContext(),
    memory.dependencies,
  );
  let finishedCode: string | null = null;
  flow.attachTransport({
    finishAuth: async (params: URLSearchParams) => {
      finishedCode = params.get('code');
      await flow.provider.saveTokens({ access_token: 'super-secret', token_type: 'Bearer' });
    },
  } as unknown as StreamableHTTPClientTransport);

  const authorize = Promise.resolve(flow.provider.redirectToAuthorization(new URL('https://auth.example.test/authorize?state=expected')));
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  assert.ok(flow.provider.redirectUrl);
  const response = await fetch(`${new URL(flow.provider.redirectUrl).href}?state=expected&code=accepted`);
  assert.equal(response.status, 200);
  const callbackPage = await response.text();
    assert.match(callbackPage, /--orange:#FF8A3D/);
    assert.match(callbackPage, /--violet:#7957D5/);
  assert.doesNotMatch(callbackPage, /super-secret/);
  await authorize;
  assert.equal(opened.length, 1);
  assert.equal(finishedCode, 'accepted');
  assert.equal(memory.stored().tokens?.access_token, 'super-secret');
  assert.equal(await flow.hasTokens(), true);

  assert.ok(flow.provider.invalidateCredentials);
  await flow.provider.invalidateCredentials('all');
  assert.equal(memory.deleted(), true);
  assert.equal(await flow.hasTokens(), false);
  flow.close();
});

test('OAuth rejects mismatched callback state and explicit authorization denial', async () => {
  const mismatchMemory = memoryDependencies();
  const mismatch = await createMcpOAuthFlowWithDependencies(
    'remote',
    'https://mcp.example.test/api',
    interactiveContext(),
    mismatchMemory.dependencies,
  );
  mismatch.attachTransport({ finishAuth: async () => undefined } as unknown as StreamableHTTPClientTransport);
  const pending = Promise.resolve(mismatch.provider.redirectToAuthorization(new URL('https://auth.example.test/authorize?state=expected')));
  const rejected = assert.rejects(pending, /state mismatch/i);
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  assert.ok(mismatch.provider.redirectUrl);
  const response = await fetch(`${new URL(mismatch.provider.redirectUrl).href}?state=wrong&code=accepted`);
  assert.equal(response.status, 400);
  await rejected;
  mismatch.close();

  const denied = await createMcpOAuthFlowWithDependencies(
    'remote',
    'https://mcp.example.test/api',
    interactiveContext(false),
    memoryDependencies().dependencies,
  );
  await assert.rejects(
    () => Promise.resolve(denied.provider.redirectToAuthorization(new URL('https://auth.example.test/authorize?state=expected'))),
    /denied by user/i,
  );
  denied.close();
});
