/**
 * Heap snapshot: optional garbage collection, then HeapProfiler.takeHeapSnapshot with each
 * addHeapSnapshotChunk written to heap.heapsnapshot in arrival order, straight to disk (the
 * snapshot is never held in memory). Prints its byte size. Option: HEAP_COLLECT_GARBAGE.
 */
import { join } from 'node:path';
import type { Payload, Session } from '../cdp-connection.mjs';
import { openSink } from './stream-io.mjs';
import { flagOption, say } from './stream-options.mjs';

export async function run(cdp: Session) {
  const collect = flagOption('HEAP_COLLECT_GARBAGE', false);
  await cdp.send('HeapProfiler.enable');
  if (collect) await cdp.send('HeapProfiler.collectGarbage');

  const file = join(cdp.outputDir, 'heap.heapsnapshot');
  const sink = openSink(file);
  let chunks = 0;
  let failed: unknown;
  const onChunk = ({ chunk }: Payload) => {
    // A sync write keeps chunks in event order; a write failure surfaces after the snapshot call.
    try {
      sink.writeText(String(chunk));
      chunks++;
    } catch (error) {
      failed ??= error;
    }
  };
  cdp.on('HeapProfiler.addHeapSnapshotChunk', onChunk);
  try {
    await cdp.send(
      'HeapProfiler.takeHeapSnapshot',
      { reportProgress: false, captureNumericValue: true },
      undefined,
      { timeoutMs: 600000 }
    );
  } finally {
    cdp.off('HeapProfiler.addHeapSnapshotChunk', onChunk);
    sink.close();
    await cdp.send('HeapProfiler.disable').catch(() => {});
  }
  if (failed) throw failed;
  say(
    `[METRIC] HEAP_SNAPSHOT bytes=${sink.bytes} chunks=${chunks} garbageCollected=${collect}`
  );
  say(`[ARTIFACT] HEAP_SNAPSHOT ${file}`);
}
