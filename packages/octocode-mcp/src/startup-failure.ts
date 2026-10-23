/** How long a failed start waits for the client's first request. */
export const STARTUP_FAILURE_REPLY_MS = 10_000;

/** The JSON-RPC id of a request line; undefined for anything else. */
function requestId(line: string): string | number | undefined {
  let message: unknown;
  try {
    message = JSON.parse(line);
  } catch {
    return undefined;
  }
  if (typeof message !== 'object' || message === null || !('id' in message)) {
    return undefined;
  }
  const { id } = message;
  return typeof id === 'string' || typeof id === 'number' ? id : undefined;
}

/**
 * A failed start answers the client's first request (its `initialize`) with
 * the reason, then exits, so the client shows why instead of a closed
 * connection. stderr keeps the same line for logs.
 */
export function failStartup(message: string): void {
  process.stderr.write(`${message}\n`);
  let done = false;
  let replied = false;
  const exit = (): void => {
    if (done) return;
    done = true;
    clearTimeout(timer);
    process.exit(1);
  };
  const timer = setTimeout(exit, STARTUP_FAILURE_REPLY_MS);
  timer.unref?.();
  let buffer = '';
  process.stdin.setEncoding('utf8');
  process.stdin.on('data', (chunk: string) => {
    buffer += chunk;
    const lines = buffer.split('\n');
    buffer = lines.pop() ?? '';
    for (const line of lines) {
      const id = replied ? undefined : requestId(line);
      if (id === undefined) continue;
      const error = { code: -32603, message };
      process.stdout.write(
        `${JSON.stringify({ jsonrpc: '2.0', id, error })}\n`,
        exit
      );
      replied = true;
      return;
    }
  });
  process.stdin.once('end', exit);
  process.stdin.once('error', exit);
}
