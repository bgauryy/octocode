import { startNativeMcp } from './native/index.mjs';

async function startServer(): Promise<void> {
  await startNativeMcp();
}

startServer().catch((error: unknown) => {
  const message =
    error instanceof Error ? error.message : String(error ?? 'unknown');
  process.stderr.write(`Server initialization failed: ${message}\n`);
  process.exit(1);
});
