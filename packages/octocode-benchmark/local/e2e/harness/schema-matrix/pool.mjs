// Workers sized to the machine: each owns a hermetic world (home, provider mock,
// registry proxy) and one MCP server, so nothing is queued behind another's state.
import os from 'node:os';

/** Memory one worker and its running processes hold at peak, measured on this harness. */
const WORKER_BYTES = 300 * 1024 * 1024;

/** One per CPU unless the machine's memory says fewer; never a constant. */
export function workerCount({ cpus = os.availableParallelism(), memory = os.totalmem(), requested = process.env.OCTOCODE_TEST_WORKERS } = {}) {
  const asked = Number(requested);
  if (Number.isInteger(asked) && asked > 0) return asked;
  return Math.max(1, Math.min(cpus, Math.floor(memory / WORKER_BYTES)));
}

/** Drain `queue` with every worker; a worker stops only when the queue is empty. */
export async function drain(workers, queue, runItem) {
  await Promise.all(workers.map(async worker => {
    for (let item = queue.shift(); item !== undefined; item = queue.shift()) await runItem(worker, item);
  }));
}
