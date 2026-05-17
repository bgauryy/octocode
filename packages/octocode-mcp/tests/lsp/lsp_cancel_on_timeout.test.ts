/**
 * T1.3 — LSP requests must send $/cancelRequest on per-request timeout.
 *
 * Why: without cancellation the language server keeps doing the work
 * (and holding memory / locks) even after our client gave up. This is
 * particularly bad for tsserver, where the same agent typically issues
 * many requests in a row.
 *
 * We test by spying on `MessageConnection.sendRequest` and asserting
 * the `CancellationToken` passed as the 3rd argument is `cancelled`
 * after the timeout fires.
 */
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
        // Return a promise that NEVER resolves — forces the timeout path.
        return new Promise(() => {});
      }
    ),
  } as unknown as MessageConnection;

  return { connection, capturedToken };
}

describe('T1.3 — sendRequestWithCancellationOnTimeout', () => {
  it('passes a CancellationToken to sendRequest', async () => {
    const { connection, capturedToken } = makeFakeConnection();
    // Fire and don't await; we only want to verify the call signature.
    const p = sendRequestWithCancellationOnTimeout(
      connection,
      'textDocument/definition',
      {},
      10
    );
    // Allow the microtask that calls sendRequest to run.
    await Promise.resolve();
    expect(capturedToken.value).toBeDefined();
    expect(capturedToken.value!.isCancellationRequested).toBe(false);
    // Drain the rejected timeout so vitest doesn't warn about unhandled.
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
