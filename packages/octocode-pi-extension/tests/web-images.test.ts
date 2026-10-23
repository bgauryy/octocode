import fs from 'node:fs';
import { describe, expect, it } from 'vitest';
import { unreadReason } from '../src/web/fetch.js';
import { IMAGE_MAX_CHARS } from '../src/shared/spill.js';
import { registerWebTool, webSummary } from '../src/web/web.js';
import { fakePermission, fakePi } from './fake-pi.js';
import { asText, noisyPng, pixelSize } from './helpers.js';
import { fetchUrl, privateOk, serve, type ToolResult, useWebServers, webTool } from './web-harness.js';

useWebServers();

const GIF_1X1 = Buffer.from('R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7', 'base64');
const png = noisyPng(512);
const limit100 = { inputLimits: { images: { resize: { maxWidth: 100, maxHeight: 100 } } } };
const imageServer = (hits: string[] = []) =>
  serve((req, res) => {
    hits.push(req.url!);
    const send = (status: number, type: string, body: Buffer | string, headers: Record<string, string | number> = {}) => res.writeHead(status, { 'content-type': type, ...headers }).end(body);
    if (req.url === '/small.png') send(200, 'image/png', noisyPng(8));
    else if (req.url === '/big.png') send(200, 'image/png', png);
    else if (req.url === '/dot.gif') send(200, 'image/gif', GIF_1X1);
    else if (req.url === '/mislabeled.jpg') send(200, 'image/jpeg', noisyPng(8));
    else if (req.url === '/fake.png') send(200, 'image/png', '<html>not an image</html>');
    else if (req.url === '/missing.png') send(404, 'image/png', 'nope');
    else if (req.url === '/huge.png') send(200, 'image/png', Buffer.alloc(6 * 1024 * 1024, 1), { 'content-length': 6 * 1024 * 1024 });
    else if (req.url === '/icon.svg') send(200, 'image/svg+xml', '<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script><circle r="4"/></svg>');
    else send(200, 'image/bmp', Buffer.alloc(32, 2));
  });
const savedFile = (result: ToolResult) => /Original saved to (\S+?\.(?:png|jpg|gif|webp))\./.exec(result.content[0].text!)?.[1];

describe('web images', () => {
  it('returns an image within the budget as an image part, with the URL, type, size and the saved original', async () => {
    const base = await imageServer();
    const result = (await webTool().execute('1', { url: `${base}/small.png` }));
    expect(result.content.map((part) => part.type)).toEqual(['text', 'image']);
    expect(result.content[0].text).toMatch(new RegExp(`^${base.replace(/[.]/g, '\\.')}/small\\.png: image/png image, \\d+(\\.\\d)?(B|KB|MB)\\. Original saved to \\S+\\.png\\.$`));
    expect(result.content[1]).toEqual({ type: 'image', data: noisyPng(8).toString('base64'), mimeType: 'image/png' });
    expect(fs.readFileSync(savedFile(result)!)).toEqual(noisyPng(8));
    expect(result.details).toMatchObject({ kind: 'image', mimeType: 'image/png', bytes: noisyPng(8).length, file: savedFile(result) });
    expect(webSummary(result.details)).toMatch(/^Fetched image\/png · \d/);
  });

  it('resizes an image over the context budget to the model limit, keeps the original and adds the dimension note', async () => {
    const base = await imageServer();
    const result = (await webTool([], fakePermission('full'), limit100).execute('1', { url: `${base}/big.png` }));
    expect(result.content.map((part) => part.type)).toEqual(['text', 'image', 'text']);
    const image = result.content[1];
    expect(image.data!.length).toBeLessThanOrEqual(IMAGE_MAX_CHARS);
    expect(pixelSize(image.data!)).toEqual({ width: 100, height: 100 });
    expect(result.content[2].text).toContain('original 512x512, displayed at 100x100');
    expect(result.content[2].text).toContain(`Original saved to ${savedFile(result)}.`);
    expect(fs.readFileSync(savedFile(result)!)).toEqual(png);
    // One saved copy: the fitted note names the file the first line names.
    expect(result.details).toMatchObject({ kind: 'image', file: savedFile(result) });
  }, 120_000); // CPU-bound wasm resize: seconds alone, several times that on a loaded machine

  it('fits an over-budget image to the default limit when the model says nothing about images', async () => {
    const base = await imageServer();
    const result = (await webTool().execute('1', { url: `${base}/big.png` }));
    const image = result.content.find((part) => part.type === 'image')!;
    expect(image.data!.length).toBeLessThanOrEqual(IMAGE_MAX_CHARS);
    expect(pixelSize(image.data!).width).toBeLessThanOrEqual(1568);
  }, 120_000); // CPU-bound wasm resize: seconds alone, several times that on a loaded machine

  it('passes a GIF through under the budget and names the real format when the header is wrong', async () => {
    const base = await imageServer();
    const gif = (await webTool().execute('1', { url: `${base}/dot.gif` }));
    expect(gif.content[1]).toEqual({ type: 'image', data: GIF_1X1.toString('base64'), mimeType: 'image/gif' });
    const jpg = (await webTool().execute('1', { url: `${base}/mislabeled.jpg` }));
    expect(jpg.content[1]).toMatchObject({ type: 'image', mimeType: 'image/png' });
    expect(savedFile(jpg)).toMatch(/\.png$/);
  });

  it('refuses a body that is not the image its header claims, instead of sending it on as an image', async () => {
    const base = await imageServer();
    const notice = asText(await fetchUrl(`${base}/fake.png`, undefined, privateOk));
    expect(notice).toMatch(/is image\/png by its header, but the body is not a PNG, JPEG, GIF or WebP image; web cannot read it/);
    expect(unreadReason(notice)).toBe('unsupported');
    const result = (await webTool().execute('1', { url: `${base}/fake.png` }));
    expect(result.content.map((part) => part.type)).toEqual(['text']);
    expect(result.details).toEqual({ kind: 'notice', reason: 'unsupported' });
  });
});

describe('web images over the cap, other types and caching', () => {
  it('refuses an image over the 5 MB read cap clearly, by Content-Length or by counting, and reports a missing one as an HTTP error', async () => {
    const hits: string[] = [];
    const base = await imageServer(hits);
    const huge = asText(await fetchUrl(`${base}/huge.png`, undefined, privateOk));
    expect(huge).toMatch(/is image\/png, 6\.0MB, over web's 5 MB limit; web cannot read it\. Download it with bash \(curl -o\)/);
    expect(unreadReason(huge)).toBe('unsupported');
    // No Content-Length (chunked): the reader stops at the cap and the notice names only the limit.
    const base2 = await serve((_req, res) => {
      res.writeHead(200, { 'content-type': 'image/webp' });
      const chunk = Buffer.alloc(1024 * 1024, 1);
      const write = (count: number) => {
        while (count < 8) {
          count += 1;
          if (!res.write(chunk)) {
            res.once('drain', () => write(count));
            return;
          }
        }
        res.end();
      };
      res.on('error', () => undefined);
      write(0);
    });
    expect(asText(await fetchUrl(`${base2}/chunked.webp`, undefined, privateOk))).toMatch(/is image\/webp, over web's 5 MB limit; web cannot read it/);
    await expect(fetchUrl(`${base}/missing.png`, undefined, privateOk)).rejects.toThrow(/HTTP 404 .* for http/);
  });

  it('reports a connection that drops while an image downloads as a failed fetch', async () => {
    const base = await serve((_req, res) => {
      res.writeHead(200, { 'content-type': 'image/png', 'content-length': 1000 });
      res.write(Buffer.alloc(10));
      setTimeout(() => res.destroy(), 20);
    });
    await expect(fetchUrl(`${base}/cut.png`, undefined, privateOk)).rejects.toThrow();
  });

  it('keeps other binary types a notice, and reads SVG as untrusted XML text, never as an image', async () => {
    const base = await imageServer();
    const bmp = (await webTool().execute('1', { url: `${base}/photo.bmp` }));
    expect(bmp.content.map((part) => part.type)).toEqual(['text']);
    expect(bmp.details).toEqual({ kind: 'notice', reason: 'unsupported' });
    const svg = (await webTool().execute('1', { url: `${base}/icon.svg` }));
    expect(svg.content.map((part) => part.type)).toEqual(['text']);
    expect(svg.content[0].text).toContain('<circle r="4"/>');
    expect(svg.details).toMatchObject({ kind: 'fetch' });
  });

  it('does not cache images (the saved file may be swept) while text pages stay cached', async () => {
    const hits: string[] = [];
    const base = await imageServer(hits);
    await fetchUrl(`${base}/small.png`, undefined, privateOk);
    await fetchUrl(`${base}/small.png`, undefined, privateOk);
    expect(hits).toEqual(['/small.png', '/small.png']);
    await fetchUrl(`${base}/icon.svg`, undefined, privateOk);
    await fetchUrl(`${base}/icon.svg`, undefined, privateOk);
    expect(hits.filter((hit) => hit === '/icon.svg')).toHaveLength(1);
  });

  it('describes images in the tool description', () => {
    const fake = fakePi();
    fake.pi.setActiveTools([]);
    registerWebTool(fake.pi, fakePermission('full'));
    const description: string = fake.tools.get('web').description;
    expect(description).toMatch(/PNG, JPEG, GIF or WebP URL returns the image itself/);
    expect(description).toContain('images are not');
  });
});

