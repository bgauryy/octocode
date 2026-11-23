// A file:// replay uses the same event fold and query engine as the loopback viewer.
import { openSavedGraph } from '../assets/live.ts';
import { byId } from '../assets/live-dom.ts';
import type { LoggedEvent, QueryAnswer } from '../assets/live-state.ts';
import type { WorkerRequest } from './worker.ts';

function takeBytes(id: string) {
  const node = byId(id);
  const text = node.textContent ?? '';
  const bytes = Uint8Array.fromBase64
    ? Uint8Array.fromBase64(text)
    : Uint8Array.from(atob(text), char => char.charCodeAt(0));
  node.remove();
  return bytes;
}

let worker: Worker | undefined;
const view = openSavedGraph(text => {
  if (!worker) {
    const source = byId('saved-worker');
    const url = URL.createObjectURL(
      new Blob([source.textContent ?? ''], { type: 'text/javascript' })
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
    const root: unknown = JSON.parse(byId('saved-root').textContent ?? '');
    worker.postMessage({ bytes, root } satisfies WorkerRequest, [bytes.buffer]);
    worker.addEventListener('message', ({ data }: MessageEvent<QueryAnswer>) =>
      view.answer(data)
    );
    worker.addEventListener('error', event =>
      view.answer({
        ok: false,
        error: event.message || 'The saved query worker failed to start.',
        text,
      })
    );
  }
  worker.postMessage({ text } satisfies WorkerRequest, []);
});

async function load() {
  const stream = new Blob([takeBytes('saved-events')])
    .stream()
    .pipeThrough(new DecompressionStream('gzip'))
    .pipeThrough(new TextDecoderStream());
  let pending: string[] = [],
    batch: LoggedEvent[] = [],
    lastDraw = performance.now();
  for await (const text of stream) {
    const lines = text.split('\n');
    for (let i = 0; i < lines.length - 1; i++) {
      pending.push(lines[i]);
      const line = pending.join('');
      pending = [];
      // events.jsonl: one logged event per line.
      if (line.trim()) batch.push(JSON.parse(line) as LoggedEvent);
    }
    pending.push(lines.at(-1) ?? '');
    if (batch.length && performance.now() - lastDraw > 32) {
      view.events(batch);
      batch = [];
      await new Promise(resolve => requestAnimationFrame(resolve));
      lastDraw = performance.now();
    }
  }
  const last = pending.join('');
  if (last.trim()) batch.push(JSON.parse(last) as LoggedEvent);
  view.events(batch);
}

load().catch((error: Error) => {
  const status = byId('empty');
  status.hidden = false;
  status.textContent = `Could not replay this saved graph: ${error.message}`;
});
