import { describe, expect, it, vi } from 'vitest';
import type {
  CancellationToken,
  MessageConnection,
} from 'vscode-jsonrpc/node.js';
import { sendRequestWithCancellationOnTimeout } from '../../src/lsp/cancellableRequest.js';

function makeFakeConnection(): {
  connection: MessageConnection;
  capturedToken: { value: CancellationToken | undefined };
} {
  const capturedToken: { value: CancellationToken | undefined } = {
    value: undefined,
  };

  const connection = {
    sendRequest: vi.fn(
      (_method: string, _params: unknown, token?: CancellationToken): any => {
        capturedToken.value = token;
        return new Promise(() => {});
      }
    ),
  } as unknown as MessageConnection;

  return { connection, capturedToken };
}

describe('T1.3 — sendRequestWithCancellationOnTimeout', () => {
  it('passes a CancellationToken to sendRequest', async () => {
    const { connection, capturedToken } = makeFakeConnection();
    const p = sendRequestWithCancellationOnTimeout(
      connection,
      'textDocument/definition',
      {},
      10
    );
    await Promise.resolve();
    expect(capturedToken.value).toBeDefined();
    expect(capturedToken.value!.isCancellationRequested).toBe(false);
    await expect(p).rejects.toThrow(/timed out/);
  });

  it('cancels the token (and rejects) when the request exceeds the timeout', async () => {
    const { connection, capturedToken } = makeFakeConnection();
    const p = sendRequestWithCancellationOnTimeout(
      connection,
      'textDocument/references',
      {},
      10
    );
    await expect(p).rejects.toThrow(/timed out after 10ms/);
    expect(capturedToken.value).toBeDefined();
    expect(capturedToken.value!.isCancellationRequested).toBe(true);
  });

  it('does NOT cancel the token when the request resolves in time', async () => {
    const capturedToken: { value: CancellationToken | undefined } = {
      value: undefined,
    };
    const connection = {
      sendRequest: vi.fn(
        (_method: string, _params: unknown, token?: CancellationToken) => {
          capturedToken.value = token;
          return Promise.resolve('ok');
        }
      ),
    } as unknown as MessageConnection;

    const result = await sendRequestWithCancellationOnTimeout<string>(
      connection,
      'textDocument/definition',
      {},
      1000
    );

    expect(result).toBe('ok');
    expect(capturedToken.value).toBeDefined();
    expect(capturedToken.value!.isCancellationRequested).toBe(false);
  });
});
