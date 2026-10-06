import { startNativeMcp } from './native/index.js';

/** How long a failed start waits for the client's first request. */
const STARTUP_FAILURE_REPLY_MS = 10_000;

/**
 * A failed start answers the client's first request (its `initialize`) with
 * the reason, then exits, so the client shows why instead of a closed
 * connection. stderr keeps the same line for logs.
 */
function failStartup(message: string): void {
  process.stderr.write(`${message}\n`);
  let done = false;
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
    let end: number;
    while (!done && (end = buffer.indexOf('\n')) >= 0) {
      const line = buffer.slice(0, end);
      buffer = buffer.slice(end + 1);
      let id: unknown;
      try {
        id = (JSON.parse(line) as { id?: unknown } | null)?.id;
      } catch {
        continue;
      }
      if (typeof id !== 'string' && typeof id !== 'number') continue;
      const error = { code: -32603, message };
      process.stdout.write(
        `${JSON.stringify({ jsonrpc: '2.0', id, error })}\n`,
        exit
      );
      return;
    }
  });
  process.stdin.once('end', exit);
  process.stdin.once('error', exit);
}

startNativeMcp().catch((error: unknown) => {
  const message =
    error instanceof Error ? error.message : String(error ?? 'unknown');
  failStartup(`Server initialization failed: ${message}`);
});
