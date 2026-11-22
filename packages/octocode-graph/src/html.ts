import { createInterface } from 'node:readline';
import { checkRunHeaders } from './runtime/lib/run-output.mjs';
import { createReadStream } from 'node:fs';
import { readFile, stat } from 'node:fs/promises';
import { join } from 'node:path';
import { createGzip } from 'node:zlib';
import { writeNewFile } from './output.js';

async function* encoded(file: string) {
  let carry = Buffer.alloc(0);
  for await (const bytes of createReadStream(file).compose(createGzip())) {
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
  async function* document() {
    yield before;
    yield `<script id="saved-root" type="application/json">${JSON.stringify(replayRoot ?? null).replaceAll('<', '\\u003c')}</script>`;
    for (const [id, file] of [
      ['saved-events', 'events.jsonl'],
      ['saved-data', 'data.json'],
    ] as const) {
      yield `<script type="application/octet-stream" id="${id}">`;
      yield* encoded(join(run, file));
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
