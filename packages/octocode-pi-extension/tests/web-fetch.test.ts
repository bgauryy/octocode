import { describe, expect, it } from 'vitest';
import { language, pageFetcher, unreadReason } from '../src/web/fetch.js';
import { asText } from './helpers.js';
import { fetchUrl, privateOk, serve, useWebServers } from './web-harness.js';

useWebServers();

describe('fetchUrl reading a local server', () => {
  it('converts HTML with its title and entities, pretty-prints JSON and passes other text through', async () => {
    const base = await serve((req, res) => {
      if (req.url === '/page') {
        res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
        res.end('<html><head><title>Caf&eacute; &amp; &#x41;&#66;</title></head><body><h2>Menu</h2><p>Tea &lt;hot&gt; &#0; &bogus;</p></body></html>');
      } else if (req.url === '/json') {
        res.writeHead(200, { 'content-type': 'application/json' });
        res.end('{"a":[1,2]}');
      } else if (req.url === '/broken-json') {
        res.writeHead(200, { 'content-type': 'application/json' });
        res.end('{not json');
      } else {
        res.writeHead(200, { 'content-type': 'text/plain' });
        res.end('plain body');
      }
    });
    const page = await fetchUrl(`${base}/page`, undefined, privateOk);
    expect(page).toMatch(/^# Caf&eacute; & AB\nSource: http:\/\/127\.0\.0\.1:\d+\/page/);
    expect(page).toContain('## Menu');
    expect(page).toContain('Tea <hot> &#0; &bogus;');
    expect(await fetchUrl(`${base}/json`, undefined, privateOk)).toBe('{\n  "a": [\n    1,\n    2\n  ]\n}');
    expect(await fetchUrl(`${base}/broken-json`, undefined, privateOk)).toBe('{not json');
    expect(await fetchUrl(`${base}/text`, undefined, privateOk)).toBe('plain body');
  });

  it('describes binary bodies instead of decoding them, and reports HTTP errors', async () => {
    const base = await serve((req, res) => {
      if (req.url === '/logo.bmp') {
        res.writeHead(200, { 'content-type': 'image/bmp' });
        res.end(Buffer.alloc(64, 1));
      } else if (req.url === '/missing.pdf') {
        res.writeHead(404, { 'content-type': 'application/pdf' });
        res.end('nope');
      } else {
        res.writeHead(500, { 'content-type': 'text/html' });
        res.end('<p>oops</p>');
      }
    });
    const logo = asText(await fetchUrl(`${base}/logo.bmp`, undefined, privateOk));
    expect(logo).toMatch(/is image\/bmp, not text; web cannot read it/);
    expect(unreadReason(logo)).toBe('unsupported');
    expect(unreadReason('plain body')).toBeUndefined();
    await expect(fetchUrl(`${base}/missing.pdf`, undefined, privateOk)).rejects.toThrow(/HTTP 404 .* for http/);
    await expect(fetchUrl(`${base}/error`, undefined, privateOk)).rejects.toThrow(/HTTP 500/);
  });

  it('stops reading a body at the 5 MB cap', async () => {
    const chunk = 'x'.repeat(1024 * 1024);
    let written = 0;
    const base = await serve((_req, res) => {
      res.writeHead(200, { 'content-type': 'text/plain' });
      // Far more than the cap: the reader must stop early instead of buffering it all.
      const write = () => {
        while (written < 64) {
          written += 1;
          if (!res.write(chunk)) {
            res.once('drain', write);
            return;
          }
        }
        res.end();
      };
      res.on('error', () => undefined);
      write();
    });
    const body = asText(await fetchUrl(`${base}/big`, undefined, privateOk));
    expect(body.length).toBeGreaterThanOrEqual(5 * 1024 * 1024);
    expect(body.length).toBeLessThan(8 * 1024 * 1024);
  });
});

describe('fetchUrl retries, cache and language', () => {
  it('retries transient statuses, honours Retry-After, and names the network cause', async () => {
    let hits = 0;
    const base = await serve((req, res) => {
      hits += 1;
      if (req.url === '/flaky' && hits < 3) {
        res.writeHead(503, { 'retry-after': '0' });
        res.end('busy');
        return;
      }
      if (req.url === '/reset') {
        req.socket.destroy();
        return;
      }
      res.writeHead(200, { 'content-type': 'text/plain' });
      res.end(`ok after ${hits}`);
    });
    expect(await fetchUrl(`${base}/flaky`, undefined, privateOk)).toBe('ok after 3');
    await expect(fetchUrl(`${base}/reset`, undefined, privateOk)).rejects.toThrow(/^Could not fetch http:\/\/127\.0\.0\.1:\d+\/reset: \S+.* \(tried 3 times\)\. The site may block/);
  });

  it('caches a page for a while and sends the system language as Accept-Language', async () => {
    const languages: string[] = [];
    const base = await serve((req, res) => {
      languages.push(String(req.headers['accept-language']));
      res.writeHead(200, { 'content-type': 'text/plain' });
      res.end(`visit ${languages.length}`);
    });
    expect(await fetchUrl(`${base}/c`, undefined, privateOk)).toBe('visit 1');
    expect(await fetchUrl(`${base}/c`, undefined, privateOk)).toBe('visit 1');
    expect(languages).toEqual([language()]);
  });

  it('keeps each fetcher\'s pages to itself: another one (a reloaded web tool) reads the site again', async () => {
    let visits = 0;
    const base = await serve((_req, res) => {
      res.writeHead(200, { 'content-type': 'text/plain' });
      res.end(`visit ${++visits}`);
    });
    expect(await fetchUrl(`${base}/own`, undefined, privateOk)).toBe('visit 1');
    expect(await pageFetcher()(`${base}/own`, undefined, privateOk)).toBe('visit 2');
    expect(await fetchUrl(`${base}/own`, undefined, privateOk)).toBe('visit 1');
  });

  it('asks for the system language first and English as the fallback', () => {
    expect(language('en-US')).toBe('en-US,en;q=0.9');
    expect(language('he-IL')).toBe('he-IL,he;q=0.9,en;q=0.8');
    expect(language('de')).toBe('de,en;q=0.9');
    expect(language('en')).toBe('en');
  });

  it('bounds the cache by total size and evicts the least recently used page', async () => {
    const hits: Record<string, number> = {};
    const body = 'x'.repeat(4_500_000);
    const base = await serve((req, res) => {
      hits[req.url!] = (hits[req.url!] ?? 0) + 1;
      res.writeHead(200, { 'content-type': 'text/plain' });
      res.end(body);
    });
    for (let page = 0; page < 7; page++) await fetchUrl(`${base}/p${page}`, undefined, privateOk); // ~31.5 MB
    await fetchUrl(`${base}/p0`, undefined, privateOk); // a hit: p0 is now the most recently used
    await fetchUrl(`${base}/p7`, undefined, privateOk); // over 32 MB: p1 goes
    await fetchUrl(`${base}/p0`, undefined, privateOk);
    await fetchUrl(`${base}/p1`, undefined, privateOk);
    expect([hits['/p0'], hits['/p1'], hits['/p7']]).toEqual([1, 2, 1]);
  }, 30_000);
});

describe('fetchUrl private addresses and page metadata', () => {
  it('refuses a redirect from an approved private address, and reports one to another host', async () => {
    const base = await serve((req, res) => {
      if (req.url === '/old') {
        res.writeHead(301, { location: '/new' });
        res.end();
        return;
      }
      if (req.url === '/away') {
        res.writeHead(302, { location: 'https://elsewhere.example/landing' });
        res.end();
        return;
      }
      res.writeHead(200, { 'content-type': 'text/plain' });
      res.end('new place');
    });
    // An approved private address is read for the first hop only: its own redirects are checked like any other.
    await expect(fetchUrl(`${base}/old`, undefined, privateOk)).rejects.toThrow('Blocked private or non-public address');
    const away = asText(await fetchUrl(`${base}/away`, undefined, privateOk));
    expect(away).toMatch(/redirects \(HTTP 302\) to another host: https:\/\/elsewhere\.example\/landing/);
    expect(away).toContain('server-supplied and not verified');
    expect(unreadReason(away)).toBe('redirect');
  });

  it('marks static HTML, warns about JavaScript apps and shows page metadata first', async () => {
    const base = await serve((_req, res) => {
      res.writeHead(200, { 'content-type': 'text/html' });
      res.end(
        '<html lang="en"><head><title>Plans</title><meta property="og:price:amount" content="17"><script type="application/ld+json">{"@context":"https://schema.org","@type":"Offer","price":"17","priceCurrency":"USD"}</script>' +
          '<script id="wix-warmup-data">{}</script></head><body><main>' + '<p>Business plan with everything you need to grow.</p>'.repeat(20) + '<p hidden>$159 .77 placeholder</p></main></body></html>',
      );
    });
    const page = await fetchUrl(`${base}/plans`, undefined, privateOk);
    expect(page).toMatch(/^# Plans\nSource: http:\/\/127\.0\.0\.1:\d+\/plans · static HTML · lang en\n\nNote: static HTML of a JavaScript app/);
    expect(page).toContain('## Page metadata (from the HTML; untrusted)\nog:price:amount: 17\nJSON-LD: {"@type":"Offer","price":"17","priceCurrency":"USD"}');
    expect(page).not.toContain('$159');
  });

  it('refuses private addresses unless allowed', async () => {
    const base = await serve((_req, res) => res.end('secret'));
    await expect(fetchUrl(`${base}/`, undefined, {})).rejects.toThrow(/Blocked private/);
  });
});

