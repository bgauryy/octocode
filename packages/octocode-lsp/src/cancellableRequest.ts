import {
  CancellationTokenSource,
  type MessageConnection,
} from 'vscode-jsonrpc/node.js';

const LSP_REQUEST_TIMEOUT_MS = 30_000;

export async function sendRequestWithCancellationOnTimeout<T>(
  connection: MessageConnection,
  method: string,
  params: unknown,
  timeoutMs: number = LSP_REQUEST_TIMEOUT_MS
): Promise<T> {
  const source = new CancellationTokenSource();
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
