// The worker half of the in-process CLI (cli-in-process.ts): the runtime stays loaded, so one call costs a function call, not a Node start.
import { pathToFileURL } from 'node:url';
import { parentPort, workerData } from 'node:worker_threads';
import type { MessagePort } from 'node:worker_threads';

interface Request { args: string[]; env: NodeJS.ProcessEnv; input?: string; }
interface Start { port: MessagePort; signal: SharedArrayBuffer; runtime: string; entry: string; }

const { port, signal, runtime, entry }: Start = workerData;
const { cli } = await import(pathToFileURL(runtime).href);
const flag = new Int32Array(signal);

parentPort?.on('message', async ({ args, env, input }: Request) => {
  let stdout = '';
  let stderr = '';
  for (const name of Object.keys(process.env)) delete process.env[name];
  Object.assign(process.env, env);
  const out = process.stdout.write;
  const err = process.stderr.write;
  process.stdout.write = (text: string | Uint8Array): boolean => { stdout += Buffer.from(text).toString('utf8'); return true; };
  process.stderr.write = (text: string | Uint8Array): boolean => { stderr += Buffer.from(text).toString('utf8'); return true; };
  let status = 1;
  // A pending timer keeps this thread's event loop waking: an asynchronous SQLite backup otherwise completes only after the loop sleeps out.
  const wake = setInterval(() => {}, 5);
  try {
    status = await cli.main(args, {
      write: (text: string) => { stdout += text; },
      readInput: () => Buffer.from(input ?? ''),
      entry,
    });
  } catch (error) {
    stderr += `${error instanceof Error ? error.stack ?? error.message : String(error)}\n`;
  } finally {
    clearInterval(wake);
    process.stdout.write = out;
    process.stderr.write = err;
  }
  port.postMessage({ status, stdout, stderr });
  Atomics.store(flag, 0, 1);
  Atomics.notify(flag, 0);
});
