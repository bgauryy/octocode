import { Readable } from 'node:stream';
import { readEvents } from './runtime/pipeline.ts';
import { createInterface } from 'node:readline';
import { checkRunHeaders } from './runtime/lib/run-output.ts';
import { createReadStream } from 'node:fs';
import { readFile, stat } from 'node:fs/promises';
import { join } from 'node:path';
import { createGzip } from 'node:zlib';
import { writeNewFile } from './output.ts';
import { buildSearchIndex, readSearchIndex } from './runtime/search-sidecar.ts';

// Gzip and base64 a file, or text: a file's bytes as they are, the event log as readEvents rebuilds it (its
// `rows` events store references to the chunk results, which the page cannot read).
async function* encoded(source: string | { text: string }) {
  let carry = Buffer.alloc(0);
  const input =
    typeof source === 'string'
      ? createReadStream(source)
      : Readable.from([source.text]);
  for await (const bytes of input.compose(createGzip())) {
    const combined = Buffer.concat([carry, bytes]);
    const length = combined.length - (combined.length % 3);
    yield combined.toString('base64', 0, length);
    carry = combined.subarray(length);
  }
  if (carry.length) yield carry.toString('base64');
}

const safeScript = (text: string) => text.replace(/<\/script/gi, '<\\/script');
export async function exportHtml(run: string, output: string) {
  const assets = new URL('.', import.meta.resolve('#graph-assets/live.html'));
  const asset = (file: string) => readFile(new URL(file, assets), 'utf8');
  const bundle = (file: string) =>
    readFile(new URL(import.meta.resolve(`#graph-browser/${file}`)), 'utf8');
  const [template, viewer, worker, renderer, licenses] = await Promise.all([
    asset('live.html'),
    bundle('offline.js'),
    bundle('worker.js'),
    readFile(new URL('renderer.generated.js', assets)),
    asset('renderer.generated.LICENSE.txt'),
  ]);
  let html = template;
  for (const [tag, file] of template.matchAll(
    /<link rel="stylesheet" href="([\w.-]+\.css)">/g
  )) {
    const css = await asset(file);
    html = html.replace(tag, () => `<style>${css}</style>`);
  }
  html = html.replace(/<section id="(?:chat|wiki)"[^>]*><\/section>/g, '');
  const [before, after] = html.split(
    '<script type="module" src="live.js"></script>'
  );
  if (after === undefined)
    throw new Error('Missing viewer entry in HTML template');
  const imports = JSON.stringify({
    imports: {
      './renderer.generated.js': `data:application/javascript;base64,${renderer.toString('base64')}`,
    },
  });
  const inputs = [join(run, 'events.jsonl'), join(run, 'data.json')];
  const version = async () =>
    Promise.all(
      inputs.map(async file => {
        const s = await stat(file, { bigint: true });
        return `${s.dev}/${s.ino}/${s.size}/${s.mtimeNs}`;
      })
    );
  const beforeExport = await version();
  const check = checkRunHeaders(run);
  let replayRoot: string | undefined;
  const events = createInterface({
    input: createReadStream(inputs[0]),
    crlfDelay: Infinity,
  });
  for await (const line of events)
    if (line.trim()) replayRoot = check(JSON.parse(line));
  // The page's query worker cannot read the source: the search sidecar (doc comments, every declaration) goes with it.
  const search = readSearchIndex(inputs[1]) ?? buildSearchIndex(inputs[1]);
  async function* document() {
    yield before;
    yield `<script id="saved-root" type="application/json">${JSON.stringify(replayRoot ?? null).replaceAll('<', '\\u003c')}</script>`;
    const log = readEvents(run)
      .map(e => JSON.stringify(e))
      .join('\n');
    for (const [id, source] of [
      ['saved-events', { text: `${log}\n` }],
      ['saved-data', join(run, 'data.json')],
      ['saved-search', { text: JSON.stringify(search) }],
    ] as const) {
      yield `<script type="application/octet-stream" id="${id}">`;
      yield* encoded(source);
      yield '</script>';
    }
    yield `<script type="text/plain" id="saved-worker">${safeScript(worker)}</script><script type="text/plain" id="licenses">${safeScript(licenses)}</script><script type="importmap">${imports}</script><script type="module">${safeScript(viewer)}</script>`;
    const afterExport = await version();
    if (beforeExport.some((value, i) => value !== afterExport[i]))
      throw new Error(
        'The saved graph changed during export. Retry after the build or refresh finishes.'
      );
    yield after;
  }
  await writeNewFile(output, document());
}
