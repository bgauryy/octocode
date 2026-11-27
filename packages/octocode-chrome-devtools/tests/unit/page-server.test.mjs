// The real-Chrome tests' page server takes its HTML as data: every byte arrives as given.
import test from 'node:test';
import assert from 'node:assert/strict';
import { once } from 'node:events';
import { spawnPageServer } from '../fixtures/page-server.mjs';

// Quotes, a template, a closing script tag, line separators and non-ASCII: text that would
// break or change the meaning of a server written as code around it.
const page = `<!doctype html><title>'"\`\${1}</title><script>x = "</script>\u2028\u2029é✓\\n`;
const bare = '<!doctype html><title>bare</title>';

test('the page server answers every url with the page, and a named url with its route, byte for byte', async () => {
  const web = spawnPageServer(page, { '/nometa': bare });
  try {
    const [chunk] = await once(web.stdout, 'data');
    const origin = `http://127.0.0.1:${Number(chunk)}`;
    for (const [path, expected] of [
      ['/', page],
      ['/any/path?query=1', page],
      ['/nometa', bare],
      ['/nometa?x', page],
    ]) {
      const response = await fetch(origin + path);
      assert.equal(response.headers.get('content-type'), 'text/html', path);
      assert.equal(await response.text(), expected, path);
    }
  } finally {
    web.kill();
    await once(web, 'exit');
  }
});
