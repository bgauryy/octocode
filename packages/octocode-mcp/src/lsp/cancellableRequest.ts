/**
 * Cancellable LSP request wrapper.
 *
 * When the request exceeds the timeout we cancel the JSON-RPC token,
 * which causes `vscode-jsonrpc` to dispatch `$/cancelRequest` to the
 * server (LSP 3.17 §$/cancelRequest). This stops the server from
 * holding locks / doing work after our client has already given up,
 * which matters most for tsserver where consecutive agent requests
 * tend to pile up.
 *
 * Pulled into its own module so it can be unit-tested without spinning
 * up a full language server (see lsp_cancel_on_timeout.test.ts).
 *
 * @module lsp/cancellableRequest
 */
import {
  CancellationTokenSource,
  type MessageConnection,
} from 'vscode-jsonrpc/node.js';

/** Default timeout for LSP requests (30 seconds). */
const LSP_REQUEST_TIMEOUT_MS = 30_000;

/**
 * Send an LSP request that auto-cancels on timeout.
 *
 * On timeout:
 *   1. The CancellationTokenSource is cancelled.
 *   2. vscode-jsonrpc sends `$/cancelRequest` to the server.
 *   3. The returned promise rejects with `LSP request '<method>' timed out after <ms>ms`.
 *
 * On success:
 *   - Resolves with the server's response.
 *   - The CancellationTokenSource is disposed (no $/cancelRequest sent).
 *
 * @typeParam T - Expected response type.
 */
export async function sendRequestWithCancellationOnTimeout<T>(
  connection: MessageConnection,
  method: string,
  params: unknown,
  timeoutMs: number = LSP_REQUEST_TIMEOUT_MS
): Promise<T> {
  const source = new CancellationTokenSource();
  // `setTimeout` runs synchronously inside the Promise executor, so this
  // is always assigned before the awaited race below — no `undefined` check
  // is needed in the cleanup path.
  let timeoutId!: ReturnType<typeof setTimeout>;

  const timeoutPromise = new Promise<never>((_, reject) => {
    timeoutId = setTimeout(() => {
      source.cancel();
      reject(
        new Error(`LSP request '${method}' timed out after ${timeoutMs}ms`)
      );
    }, timeoutMs);
  });

  try {
    return await Promise.race([
      connection.sendRequest<T>(method, params, source.token),
      timeoutPromise,
    ]);
  } finally {
    clearTimeout(timeoutId);
    source.dispose();
  }
}
