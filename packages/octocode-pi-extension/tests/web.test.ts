import http from 'node:http';
import type { AddressInfo } from 'node:net';
import { afterEach, describe, expect, it } from 'vitest';
import { assertPublicUrl, isBlockedIp, publicFetch, type HostLookup } from '../src/web/guard.js';
import { fetchUrl, isBinaryType } from '../src/web/fetch.js';
import { collapseLinkRuns, htmlToText, mainContent } from '../src/web/html.js';
import { parseDuckDuckGo } from '../src/web/search.js';

const servers: http.Server[] = [];
afterEach(async () => {
  await Promise.all(servers.splice(0).map((server) => new Promise((resolve) => server.close(resolve))));
});

/** A local server (listening on every interface, so `localhost` reaches it over IPv4 or IPv6). */
async function serve(handler: http.RequestListener): Promise<number> {
  const server = http.createServer(handler);
  servers.push(server);
  await new Promise<void>((resolve) => server.listen(0, resolve));
  return (server.address() as AddressInfo).port;
}

const publicLookup: HostLookup = async () => [{ address: '93.184.216.34' }];

describe('web private-address guard', () => {
  it.each([
    ['127.0.0.1', true], ['127.9.9.9', true], ['10.1.2.3', true], ['172.16.0.1', true], ['172.31.255.255', true],
    ['192.168.1.1', true], ['169.254.169.254', true], ['100.64.0.1', true], ['0.0.0.0', true], ['224.0.0.1', true],
    ['255.255.255.255', true], ['198.18.0.1', true], ['::1', true], ['::', true], ['fe80::1', true], ['fc00::1', true],
    ['fd12:3456::1', true], ['ff02::1', true], ['::ffff:127.0.0.1', true], ['::ffff:7f00:1', true], ['::ffff:169.254.169.254', true],
    ['64:ff9b::a9fe:a9fe', true], ['2002:c0a8:0101::', true], ['2001:db8::1', true], ['not-an-ip', true],
    // IPv4-mapped 0.0.0.0/8 (the ::ffff:0:0 and ::ffff:0:1 forms once slipped through), translated, compatible, NAT64, 6to4, Teredo.
    ['::ffff:0.0.0.0', true], ['::ffff:0:0', true], ['::ffff:0:1', true], ['::ffff:0.0.0.1', true], ['::ffff:10.0.0.1', true],
    ['::ffff:0:127.0.0.1', true], ['::ffff:0:7f00:1', true], ['::ffff:0:0.0.0.0', true], ['::127.0.0.1', true], ['::0.0.0.2', true],
    ['64:ff9b::127.0.0.1', true], ['64:ff9b::0.0.0.0', true], ['64:ff9b:1::1', true], ['2002:7f00:1::', true], ['2002:0:0::1', true],
    ['2001:0:7f00:1::', true], ['2001:0:4136:e378:8000:63bf:80ff:fffe', true], ['2001:0:4136:e378:8000:63bf:f7f7:f7f7', false],
    ['0.1.2.3', true], ['0:0:0:0:0:0:0:0', true], ['2130706433', true], ['0177.0.0.1', true], ['0x7f.1', true], ['127.1', true],
    ['8.8.8.8', false], ['93.184.216.34', false], ['172.32.0.1', false], ['100.128.0.1', false], ['2606:4700::1111', false], ['::ffff:8.8.8.8', false],
    ['::ffff:0:8.8.8.8', false], ['64:ff9b::8.8.8.8', false], ['2002:0808:0808::1', false],
  ])('%s blocked=%s', (ip, blocked) => {
    expect(isBlockedIp(ip)).toBe(blocked);
  });

  it.each([
    ['http://[::ffff:0.0.0.0]/', '[::ffff:0:0]'], ['http://[::ffff:0:1]/', '[::ffff:0:1]'], ['http://[::ffff:0:127.0.0.1]/', '[::ffff:0:7f00:1]'],
    ['http://[64:ff9b::7f00:1]/', '[64:ff9b::7f00:1]'], ['http://[2001:0:7f00:1::]/', '[2001:0:7f00:1::]'], ['http://[::]/', '[::]'],
    ['http://2130706433/', '127.0.0.1'], ['http://0177.0.0.1/', '127.0.0.1'], ['http://0x7f.1/', '127.0.0.1'], ['http://127.1/', '127.0.0.1'], ['http://0/', '0.0.0.0'],
  ])('WHATWG URL parsing normalizes %s to %s, which is blocked', async (href, hostname) => {
    expect(new URL(href).hostname).toBe(hostname);
    await expect(assertPublicUrl(href, publicLookup)).rejects.toThrow('Blocked private');
  });

  it('checks literal hosts, encoded IPv4 forms and every resolved address', async () => {
    await expect(assertPublicUrl('http://169.254.169.254/latest/meta-data')).rejects.toThrow('Blocked private');
    await expect(assertPublicUrl('http://[::1]:8080/')).rejects.toThrow('Blocked private');
    await expect(assertPublicUrl('http://2130706433/')).rejects.toThrow('Blocked private');
    await expect(assertPublicUrl('http://0x7f.1/')).rejects.toThrow('Blocked private');
    await expect(assertPublicUrl('file:///etc/passwd')).rejects.toThrow('http(s) URLs only');
    await expect(assertPublicUrl('https://mixed.test/', async () => [{ address: '8.8.8.8' }, { address: '10.0.0.1' }])).rejects.toThrow('Blocked private');
    await expect(assertPublicUrl('https://example.test/', publicLookup)).resolves.toBeInstanceOf(URL);
  });

  it('refuses a local URL, and a redirect from a public host to a private one', async () => {
    const port = await serve((request, response) => {
      if (request.url === '/hop') return void response.writeHead(302, { location: `http://127.0.0.1:${port}/secret` }).end();
      response.writeHead(200, { 'content-type': 'text/plain' }).end('secret');
    });
    await expect(fetchUrl(`http://127.0.0.1:${port}/secret`, undefined, {})).rejects.toThrow('OCTOCODE_WEB_ALLOW_PRIVATE=1');
    await expect(publicFetch(`http://localhost:${port}/hop`, {}, { env: {}, lookup: publicLookup })).rejects.toThrow(`Blocked private or non-public address: 127.0.0.1:${port}`);
    // The opt-out restores plain fetching, redirects included.
    expect(await fetchUrl(`http://127.0.0.1:${port}/secret`, undefined, { OCTOCODE_WEB_ALLOW_PRIVATE: '1' })).toBe('secret');
    const followed = await publicFetch(`http://localhost:${port}/hop`, {}, { env: { OCTOCODE_WEB_ALLOW_PRIVATE: '1' } });
    expect(await followed.text()).toBe('secret');
  });

  it('says when a body over 5 MB was cut, and shows cut JSON raw', async () => {
    const MB = 1024 * 1024;
    const bigJson = JSON.stringify({ items: 'x'.repeat(6 * MB) });
    const port = await serve((request, response) => {
      if (request.url === '/big.txt') return void response.writeHead(200, { 'content-type': 'text/plain' }).end('a'.repeat(6 * MB));
      if (request.url === '/big.json') return void response.writeHead(200, { 'content-type': 'application/json' }).end(bigJson);
      if (request.url === '/exact.txt') return void response.writeHead(200, { 'content-type': 'text/plain' }).end('b'.repeat(5 * MB));
      response.writeHead(200, { 'content-type': 'application/json' }).end('{"a":1}');
    });
    const env = { OCTOCODE_WEB_ALLOW_PRIVATE: '1' };
    const text = await fetchUrl(`http://127.0.0.1:${port}/big.txt`, undefined, env);
    expect(text).toMatch(/^\[Truncated: the response is over 5 MB; only the first 5 MB was read\. Download it with bash/);
    expect(text.length - text.indexOf('\n\n') - 2).toBe(5 * MB);
    const json = await fetchUrl(`http://127.0.0.1:${port}/big.json`, undefined, env);
    expect(json).toMatch(/^\[Truncated: .* and is shown raw, not pretty-printed\./);
    expect(json).toContain('\n\n{"items":"xxx');
    expect(await fetchUrl(`http://127.0.0.1:${port}/exact.txt`, undefined, env)).toBe('b'.repeat(5 * MB));
    expect(await fetchUrl(`http://127.0.0.1:${port}/small.json`, undefined, env)).toBe('{\n  "a": 1\n}');
  });
});

describe('web', () => {
  it('converts HTML to readable text', () => {
    // A `>` inside a quoted attribute must not end the tag, and icon-only list items must not leave empty bullets.
    const noisy = htmlToText(`<ul><li><a href="/x"></a></li><li>keep</li></ul><span data-mw='{"wt":"{{a}} > b"}'>real text</span>`);
    expect(noisy).toBe('- keep\n\nreal text');
    expect(noisy).not.toContain('wt');
    const text = htmlToText('<html><script>x()</script><h1>Title</h1><p>Hello &amp; <a href="/a">link</a></p><ul><li>one</li></ul></html>');
    expect(text).toBe('# Title\n\nHello & [link](/a)\n\n- one');
    // example.com: bare href, and a `<p>` whose close tag is omitted must not run into the next block.
    expect(htmlToText('<html><head><title>T</title><link rel=icon href=data:,></head><body><p>x</p></body></html>')).toBe('x');
    expect(htmlToText(`<body><p>This domain.<a href=https://iana.org/help>Learn more</a><p>Next <a href='/s'>s</a></body>`)).toBe(
      'This domain.[Learn more](https://iana.org/help)\n\nNext [s](/s)',
    );
  });

  it('recognises binary content types', () => {
    expect(isBinaryType('application/pdf')).toBe(true);
    expect(isBinaryType('image/png')).toBe(true);
    expect(isBinaryType('application/octet-stream')).toBe(true);
    expect(isBinaryType('text/html; charset=utf-8')).toBe(false);
    expect(isBinaryType('application/json')).toBe(false);
    expect(isBinaryType('application/vnd.api+json')).toBe(false);
    expect(isBinaryType('')).toBe(false);
  });

  it('parses DuckDuckGo HTML results', () => {
    const html = '<div class="result__body"><a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com">Example</a><a class="result__snippet">Snippet</a></div>';
    expect(parseDuckDuckGo(html, 5)).toEqual([{ title: 'Example', url: 'https://example.com', snippet: 'Snippet' }]);
  });
});

describe('web page helpers', () => {
  it('drops elements marked hidden, and keeps look-alike attributes', async () => {
    const { stripHidden } = await import('../src/web/html.js');
    const html = '<div hidden><p>gone <div>nested</div></p></div><p class="hidden" data-hidden="1">kept</p><span aria-hidden="true">icon</span><span aria-hidden="false">shown</span><b style="color:red; display:none">x</b><i style="visibility: hidden">y</i><input hidden value="v"><em>end</em>';
    expect(stripHidden(html)).toBe('<p class="hidden" data-hidden="1">kept</p><span aria-hidden="false">shown</span><em>end</em>');
    expect(htmlToText('<p>a</p><div hidden>b</div>')).toBe('a');
  });

  it('collects price, locale and date metadata and JSON-LD, skipping broken blocks', async () => {
    const { pageMetadata } = await import('../src/web/html.js');
    const html = '<meta property="og:title" content="T"><meta property="product:price:amount" content="29"><meta property="og:locale" content=\'he_IL\'><meta name="description"><script type="application/ld+json">{"@context":"x","price":29}</script><script type="application/ld+json">{broken</script>';
    expect(pageMetadata(html)).toBe('## Page metadata (from the HTML; untrusted)\nproduct:price:amount: 29\nog:locale: he_IL\nJSON-LD: {"price":29}');
    expect(pageMetadata('<p>none</p>')).toBe('');
    expect(pageMetadata(`<meta itemprop="price" content="${'9'.repeat(400)}">`, 60)).toMatch(/more characters cut; the rest is in the page source/);
  });

  it('notes a JavaScript shell or app, and stays quiet for a plain page', async () => {
    const { staticHtmlNote } = await import('../src/web/html.js');
    const long = 'words '.repeat(200);
    expect(staticHtmlNote('<div id="root"></div><script src="a.js"></script>', '')).toMatch(/little text; the page is probably built by JavaScript/);
    expect(staticHtmlNote('<noscript>Please enable JavaScript</noscript>', 'x')).toMatch(/asks for JavaScript or a bot check/);
    expect(staticHtmlNote('<p>tiny page</p>', 'tiny page')).toBeUndefined();
    expect(staticHtmlNote(`<script id="__NEXT_DATA__">{}</script><p>${long}</p>`, long)).toMatch(/static HTML of a JavaScript app/);
    expect(staticHtmlNote(`<script>${'x'.repeat(5000)}</script><p>${long}</p>`, long)).toMatch(/JavaScript app/);
    expect(staticHtmlNote(`<p>${long}</p>`, long)).toBeUndefined();
  });

  it('parses Bing and DuckDuckGo lite results', async () => {
    const { parseBing, parseDuckDuckGoLite } = await import('../src/web/search.js');
    const target = Buffer.from('https://example.com/x?y=1').toString('base64url');
    const bing = `<li class="b_algo"><h2><a href="https://www.bing.com/ck/a?!&amp;&amp;u=a1${target}">One &amp; only</a></h2><p class="b_lineclamp2">Snip</p></li><li class="b_algo"><h2><a href="https://direct.example/">Two</a></h2></li><li class="b_algo"><div>no link</div></li>`;
    expect(parseBing(bing, 5)).toEqual([
      { url: 'https://example.com/x?y=1', title: 'One & only', snippet: 'Snip' },
      { url: 'https://direct.example/', title: 'Two', snippet: '' },
    ]);
    expect(parseBing(bing, 1)).toHaveLength(1);
    const lite = `<table><tr><td><a rel="nofollow" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fa.example%2F" class='result-link'>A</a></td></tr><tr><td class='result-snippet'>About A</td></tr><tr><td><a class="result-link" href="https://b.example/">B</a></td></tr></table>`;
    expect(parseDuckDuckGoLite(lite, 5)).toEqual([
      { url: 'https://a.example/', title: 'A', snippet: 'About A' },
      { url: 'https://b.example/', title: 'B', snippet: '' },
    ]);
    expect(parseDuckDuckGoLite(lite, 1)).toHaveLength(1);
  });

  it('reads Retry-After as seconds or a date', async () => {
    const { retryAfterMs } = await import('../src/web/fetch.js');
    expect(retryAfterMs('3')).toBe(3000);
    expect(retryAfterMs(new Date(10_000).toUTCString(), 4_000)).toBe(6000);
    expect(retryAfterMs('soon')).toBeUndefined();
    expect(retryAfterMs(null)).toBeUndefined();
  });
});

describe('web page reduction', () => {
  const body = `<p>${'Real content sentence. '.repeat(20)}</p>`;
  it('keeps only the marked main content, through nested elements of the same tag', () => {
    const html = `<div class="sidebar"><a href="/a">Nav</a></div><div role="main"><div>${body}</div><div>tail</div></div><footer>f</footer>`;
    expect(mainContent(html)).toBe(`<div role="main"><div>${body}</div><div>tail</div></div>`);
    expect(mainContent(`<nav>x</nav><main>${body}</main>`)).toBe(`<main>${body}</main>`);
    expect(mainContent(`<article>${body}</article><aside>a</aside>`)).toBe(`<article>${body}</article>`);
    // Two articles (a feed), a tiny main, or no marker: the whole page.
    const feed = `<article>${body}</article><article>${body}</article>`;
    expect(mainContent(feed)).toBe(feed);
    expect(mainContent(`<main>short</main>${body}`)).toBe(`<main>short</main>${body}`);
    expect(mainContent(body)).toBe(body);
  });

  it('folds long runs of link-only lines and keeps short ones', () => {
    const links = (n: number) => Array.from({ length: n }, (_, i) => `- [Item ${i}](/item/${i})`).join('\n\n');
    expect(collapseLinkRuns(`Intro\n${links(10)}\nBody text`)).toBe('Intro\n[10 links omitted]\nBody text');
    expect(collapseLinkRuns(`Intro\n${links(3)}\nBody`)).toBe(`Intro\n${Array.from({ length: 3 }, (_, i) => `- [Item ${i}](/item/${i})`).join('\n')}\nBody`);
  });
});
