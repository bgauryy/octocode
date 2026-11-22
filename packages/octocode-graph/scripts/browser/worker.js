import { runQuery } from '../../src/runtime/query-api.mjs';

let graph;
self.addEventListener('message', async ({ data }) => {
  if (data.bytes) {
    const stream = new Blob([data.bytes])
      .stream()
      .pipeThrough(new DecompressionStream('gzip'));
    graph = new Response(stream).json().then(snapshot => {
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
    self.postMessage(runQuery(await graph, data.text), []);
  } catch (error) {
    self.postMessage({ ok: false, text: data.text, error: error.message }, []);
  }
});
