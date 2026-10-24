import { test } from 'node:test';
import assert from 'node:assert/strict';
import { extractLinksFromHtml, extractHeadings, extractTablesFromHtml, extractResourcesFromHtml } from '../lib/extractors.mjs';

test('extractors keep every row, read whole attribute names, keep C#, and never fake a table selector', () => {
  const many = Array.from({ length: 1200 }, (_, i) => `<a href="/p${i}">p${i}</a>`).join('');
  assert.equal(extractLinksFromHtml(many, 'https://x.test/', 'p').length, 1200);
  const [link] = extractLinksFromHtml('<a data-href="/tracker" href="/docs">Docs</a>', 'https://x.test/', 'p');
  assert.equal(link.href, 'https://x.test/docs');
  assert.deepEqual(extractResourcesFromHtml('<script src=/s.js></script>', 'https://x.test/', 'p').map((r) => r.src), ['https://x.test/s.js']);
  assert.equal(extractHeadings('# Getting started with C#\n## Closing ##', 'p')[0].text, 'Getting started with C#');
  assert.equal(extractHeadings('## Closing ##', 'p')[0].text, 'Closing');
  const [plain, named] = extractTablesFromHtml('<table><tr><td>a</td></tr></table><table id="prices"><tr><td>b</td></tr></table>', 'p');
  assert.equal(plain.selector, null);
  assert.equal(plain.position, 1);
  assert.equal(named.selector, 'table#prices');
});
