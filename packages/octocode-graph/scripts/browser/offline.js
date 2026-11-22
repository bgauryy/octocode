// A file:// replay uses the same event fold and query engine as the loopback viewer.
import { openSavedGraph } from '../../skills/octocode-graph/assets/live.js';

function takeBytes(id) {
  const node = document.getElementById(id);
  const bytes = Uint8Array.fromBase64
    ? Uint8Array.fromBase64(node.textContent)
    : Uint8Array.from(atob(node.textContent), char => char.charCodeAt(0));
  node.remove();
  return bytes;
}

let worker;
const view = openSavedGraph(text => {
  if (!worker) {
    const source = document.getElementById('saved-worker');
    const url = URL.createObjectURL(
      new Blob([source.textContent], { type: 'text/javascript' })
    );
    // The worker bundle is self-contained. A classic blob worker also starts from a file:// opaque origin.
    worker = new Worker(url);
    worker.addEventListener('message', () => URL.revokeObjectURL(url), {
      once: true,
    });
    worker.addEventListener('error', () => URL.revokeObjectURL(url), {
      once: true,
    });
    source.remove();
    const bytes = takeBytes('saved-data');
    const root = JSON.parse(document.getElementById('saved-root').textContent);
    worker.postMessage({ bytes, root }, [bytes.buffer]);
    worker.addEventListener('message', ({ data }) => view.answer(data));
    worker.addEventListener('error', event =>
      view.answer({
        ok: false,
        error: event.message || 'The saved query worker failed to start.',
        text,
      })
    );
  }
  worker.postMessage({ text }, []);
});

async function load() {
  const stream = new Blob([takeBytes('saved-events')])
    .stream()
    .pipeThrough(new DecompressionStream('gzip'))
    .pipeThrough(new TextDecoderStream());
  let pending = [],
    batch = [],
    lastDraw = performance.now();
  for await (const text of stream) {
    const lines = text.split('\n');
    for (let i = 0; i < lines.length - 1; i++) {
      pending.push(lines[i]);
      const line = pending.join('');
      pending = [];
      if (line.trim()) batch.push(JSON.parse(line));
    }
    pending.push(lines.at(-1));
    if (batch.length && performance.now() - lastDraw > 32) {
      view.events(batch);
      batch = [];
      await new Promise(resolve => requestAnimationFrame(resolve));
      lastDraw = performance.now();
    }
  }
  const last = pending.join('');
  if (last.trim()) batch.push(JSON.parse(last));
  view.events(batch);
}

load().catch(error => {
  const status = document.getElementById('empty');
  status.hidden = false;
  status.textContent = `Could not replay this saved graph: ${error.message}`;
});
