import { runQuery } from '../runtime/query-api.ts';

/** Messages from the offline page (offline.ts): the embedded graph once, then one query per message. */
export type WorkerRequest =
  { bytes: Uint8Array<ArrayBuffer>; root: unknown } | { text: string };
type SavedGraph = Parameters<typeof runQuery>[0];
// This classic worker's global scope. The browser code is type-checked with the DOM lib, where `self` is a Window.
interface WorkerScope {
  addEventListener(
    type: 'message',
    listener: (event: MessageEvent<WorkerRequest>) => void
  ): void;
  postMessage(message: unknown, transfer: Transferable[]): void;
}
const scope = self as WorkerScope;

let graph: Promise<SavedGraph>;
scope.addEventListener('message', async ({ data }) => {
  if ('bytes' in data) {
    const stream = new Blob([data.bytes])
      .stream()
      .pipeThrough(new DecompressionStream('gzip'));
    graph = new Response(stream)
      .json()
      .then((snapshot: SavedGraph & { section?: string }) => {
        if (data.root && snapshot.section && data.root !== snapshot.section)
          throw new Error(
            'The saved event log and graph describe different project roots.'
          );
        return snapshot;
      });
    // A query awaits the same outcome even if a malformed embedded export fails to load first.
    graph.catch(() => {});
    return;
  }
  try {
    scope.postMessage(runQuery(await graph, data.text), []);
  } catch (error) {
    scope.postMessage(
      { ok: false, text: data.text, error: (error as Error).message },
      []
    );
  }
});
