import { test } from 'node:test';
import assert from 'node:assert/strict';
import { cleanForAgent, detectTargetError, detectBrowserNeed } from '../lib/text.mjs';
import { classifyWorkflow } from '../lib/analyzers.mjs';

const prose = (n) => Array.from({ length: n }, (_, i) => `Paragraph ${i} explains how the API pages results and what each field means in detail for readers.`).join('\n\n');

test('a docs page that discusses errors is not an error page', () => {
  const text = `# Handling errors\n\n${prose(3)}\n\nWhen you exceed the rate limit, the API returns 403 Forbidden or access denied.\n\n${prose(30)}`;
  assert.equal(detectTargetError({ status: 200, text }), null);
});

test('short error-shaped pages are still flagged', () => {
  assert.match(detectTargetError({ status: 200, text: 'Access Denied\n\nYou do not have permission to view this page.' }), /error page/);
  assert.match(detectTargetError({ status: 200, text: '# 404 Not Found\n\nThe page is gone.' }), /404/);
  assert.match(detectTargetError({ status: 200, text: 'Rate limit exceeded. Try again later.' }), /error page/);
});

test('cleanForAgent keeps prose that precedes the first heading', () => {
  const lead = 'This release changes how authentication tokens are refreshed, and every client must update before the end of the quarter to keep working.';
  const out = cleanForAgent(`${lead}\n\n# Details\n\n${prose(3)}`);
  assert.ok(out.includes(lead), 'dropped the lead paragraph');
});

test('cleanForAgent still drops short navigation before the first heading', () => {
  const out = cleanForAgent(`Skip to main content\nDocs\nAPI\nPricing\n\n# Getting started\n\n${prose(3)}`);
  assert.ok(out.startsWith('# Getting started'));
});

test('cleanForAgent does not cut at a mid-document copyright or pager line', () => {
  const body = `# License\n\n${prose(2)}\n\n© 2024 Example Organization grants the following rights.\n\n${prose(2)}\n\n[Previous versions are listed below]\n\n${prose(6)}`;
  const out = cleanForAgent(body);
  assert.ok(out.includes('Paragraph 5 explains'), 'lost text after a mid-document marker');
});

test('cleanForAgent still drops a trailing footer', () => {
  const out = cleanForAgent(`# Guide\n\n${prose(8)}\n\n[Previous: Install](../install)\n\n© 2024 Example Organization\nPrivacy\nTerms`);
  assert.ok(!out.includes('Privacy'));
  assert.ok(out.includes('Paragraph 7 explains'));
});

test('a static page with one analytics script is not an application shell', () => {
  const body = '<!doctype html><html><head><title>Example Domain</title><script src=/s.js></script></head><body><h1>Example Domain</h1><p>This domain is for use in documentation examples.</p></body></html>';
  assert.equal(detectBrowserNeed({ status: 200, contentType: 'text/html', body, cleanText: 'Example Domain This domain is for use in documentation examples.' }), null);
});

test('framework mounts and module bundles still recommend one browser render', () => {
  const next = '<html><body><div id="__next"></div><script id="__NEXT_DATA__" type="application/json">{}</script></body></html>';
  const spa = '<html><body><div id="root"></div><script type="module" src="/assets/index.js"></script></body></html>';
  const noscript = '<html><body><noscript>You need to enable JavaScript to run this app.</noscript><script src="/a.js"></script></body></html>';
  for (const body of [next, spa, noscript]) {
    assert.match(detectBrowserNeed({ status: 200, contentType: 'text/html', body, cleanText: '' }) || '', /render once in Chrome/);
  }
});

test('workflow rules match whole words only', () => {
  assert.equal(classifyWorkflow('an explanation of the planning process'), null);
  assert.equal(classifyWorkflow('the capital city'), null);
  assert.equal(classifyWorkflow('edit the dockerfile'), null);
  assert.equal(classifyWorkflow('/docs/getting-started'), 'docs');
  assert.equal(classifyWorkflow('See pricing plans'), 'pricing');
  assert.equal(classifyWorkflow('/api/v1/users'), 'api-reference');
});
