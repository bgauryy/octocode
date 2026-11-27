import { runQuery } from '../runtime/query-api.ts';
import {
  attachSearchIndex,
  type SearchSidecar,
} from '../runtime/search-index.ts';

/** Messages from the offline page (offline.ts): the embedded graph and its search sidecar once, then one query per
 * message. */
export type WorkerRequest =
  | {
      bytes: Uint8Array<ArrayBuffer>;
      root: unknown;
      search?: Uint8Array<ArrayBuffer>;
    }
  | { text: string };
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

const gunzipJson = (bytes: Uint8Array<ArrayBuffer>) =>
  new Response(
    new Blob([bytes]).stream().pipeThrough(new DecompressionStream('gzip'))
  ).json();

let graph: Promise<SavedGraph>;
scope.addEventListener('message', async ({ data }) => {
  if ('bytes' in data) {
    // A sidecar that fails to read leaves search without doc comments, never without answers.
    const search: Promise<SearchSidecar | null> = data.search
      ? gunzipJson(data.search).catch(() => null)
      : Promise.resolve(null);
    graph = gunzipJson(data.bytes).then(
      async (snapshot: SavedGraph & { section?: string }) => {
        if (data.root && snapshot.section && data.root !== snapshot.section)
          throw new Error(
            'The saved event log and graph describe different project roots.'
          );
        attachSearchIndex(snapshot, await search);
        return snapshot;
      }
    );
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
