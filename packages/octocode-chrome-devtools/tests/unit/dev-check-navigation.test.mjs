import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { run as monitor } from '../../dist/engine/cdp-checks/live-har-monitor.mjs';
import { pageOutcome } from '../../dist/engine/cdp-checks/navigation-outcome.mjs';

const URL404 = 'https://example.test/missing';
const DNS_URL = 'https://no-such-host.invalid/';

/** A session whose Page.navigate answers `navigate` and then emits the network events of `emit`. */
function session(outputDir, navigate, emit) {
  const handlers = new Map();
  const target = {
    outputDir,
    targetInfo: { url: 'about:blank' },
    on: (event, fn) => handlers.set(event, fn),
    off: () => {},
    emit: (event, data) => handlers.get(event)?.(data, {}),
    async send(method) {
      if (method === 'Page.navigate') {
        setTimeout(() => emit(target));
        return navigate;
      }
      return {};
    },
  };
  return target;
}

async function observe(navigate, emit, url) {
  const dir = mkdtempSync(join(tmpdir(), 'octo-dev-nav-'));
  try {
    return await monitor(session(dir, navigate, emit), {
      url,
      monitorMs: 2000,
      waitUntil: 'load',
    });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

const loaded = target => target.emit('Page.loadEventFired', {});

test('the main document status and url are reported, not only the failed request', async () => {
  const observed = await observe(
    { frameId: 'main', loaderId: 'l1' },
    target => {
      // A subresource of the same page answered later with its own status.
      const document = { requestId: 'doc', frameId: 'main', type: 'Document' };
      target.emit('Network.requestWillBeSent', {
        ...document,
        request: { url: URL404, method: 'GET' },
        timestamp: 1,
      });
      target.emit('Network.responseReceived', {
        requestId: 'doc',
        type: 'Document',
        response: {
          status: 404,
          statusText: 'Not Found',
          mimeType: 'text/html',
        },
      });
      target.emit('Network.loadingFinished', { requestId: 'doc' });
      target.emit('Network.requestWillBeSent', {
        requestId: 'img',
        frameId: 'main',
        type: 'Image',
        request: { url: 'https://example.test/a.png', method: 'GET' },
        timestamp: 2,
      });
      target.emit('Network.responseReceived', {
        requestId: 'img',
        type: 'Image',
        response: { status: 200, mimeType: 'image/png' },
      });
      target.emit('Network.loadingFinished', { requestId: 'img' });
      loaded(target);
    },
    URL404
  );
  assert.deepEqual(observed.navigation.document, {
    url: URL404,
    status: 404,
    statusText: 'Not Found',
    mimeType: 'text/html',
  });
  assert.equal(observed.navigation.error, null);
});

test('the main document is the last hop of a redirect chain', async () => {
  const observed = await observe(
    { frameId: 'main', loaderId: 'l1' },
    target => {
      target.emit('Network.requestWillBeSent', {
        requestId: 'doc',
        frameId: 'main',
        type: 'Document',
        request: { url: 'http://example.test/', method: 'GET' },
        timestamp: 1,
      });
      target.emit('Network.requestWillBeSent', {
        requestId: 'doc',
        frameId: 'main',
        type: 'Document',
        request: { url: 'https://example.test/', method: 'GET' },
        redirectResponse: { status: 301 },
        timestamp: 2,
      });
      target.emit('Network.responseReceived', {
        requestId: 'doc',
        type: 'Document',
        response: { status: 200, mimeType: 'text/html' },
      });
      target.emit('Network.loadingFinished', { requestId: 'doc' });
      loaded(target);
    },
    'http://example.test/'
  );
  assert.equal(observed.navigation.document.url, 'https://example.test/');
  assert.equal(observed.navigation.document.status, 200);
});

test('a navigation Chrome refuses is reported with its error text', async () => {
  const observed = await observe(
    { frameId: 'main', errorText: 'net::ERR_NAME_NOT_RESOLVED' },
    target => {
      target.emit('Network.requestWillBeSent', {
        requestId: 'doc',
        frameId: 'main',
        type: 'Document',
        request: { url: DNS_URL, method: 'GET' },
        timestamp: 1,
      });
      target.emit('Network.loadingFailed', {
        requestId: 'doc',
        errorText: 'net::ERR_NAME_NOT_RESOLVED',
      });
      loaded(target);
    },
    DNS_URL
  );
  assert.equal(observed.navigation.error, 'net::ERR_NAME_NOT_RESOLVED');
  assert.deepEqual(observed.navigation.document, {
    url: DNS_URL,
    status: null,
    failed: 'net::ERR_NAME_NOT_RESOLVED',
  });
});

test('a chrome-error page reports the document url and status plainly, with no error-page title', () => {
  const outcome = pageOutcome(URL404, {
    navigation: {
      error: null,
      document: { url: URL404, status: 404, statusText: 'Not Found' },
    },
    page: { url: 'chrome-error://chromewebdata/', title: 'example.test' },
  });
  assert.equal(outcome.finalUrl, URL404);
  assert.equal(outcome.title, null);
  assert.equal(outcome.document.status, 404);
  assert.deepEqual(outcome.navigationErrors, []);
});

test('a page that loaded keeps its own url and title', () => {
  const outcome = pageOutcome('http://example.test/', {
    navigation: {
      error: null,
      document: { url: 'https://example.test/home', status: 200 },
    },
    page: { url: 'https://example.test/home#x', title: 'Home' },
  });
  assert.equal(outcome.finalUrl, 'https://example.test/home#x');
  assert.equal(outcome.title, 'Home');
});

test('a navigation failure counts as an error row naming the url and cause', () => {
  for (const navigation of [
    { error: 'net::ERR_NAME_NOT_RESOLVED', document: null },
    {
      error: null,
      document: {
        url: DNS_URL,
        status: null,
        failed: 'net::ERR_NAME_NOT_RESOLVED',
      },
    },
  ]) {
    const outcome = pageOutcome(DNS_URL, {
      navigation,
      page: {
        url: 'chrome-error://chromewebdata/',
        title: 'no-such-host.invalid',
      },
    });
    assert.equal(outcome.navigationErrors.length, 1);
    const [row] = outcome.navigationErrors;
    assert.equal(row.level, 'error');
    assert.equal(row.kind, 'navigation');
    assert.equal(row.url, DNS_URL);
    assert.match(row.message, /ERR_NAME_NOT_RESOLVED/);
    assert.equal(outcome.finalUrl, DNS_URL);
  }
});

test('an HTTP error status is the document status, not a navigation failure', () => {
  // Chrome refuses an empty 4xx/5xx body with ERR_HTTP_RESPONSE_CODE_FAILURE after the server answered.
  const outcome = pageOutcome(URL404, {
    navigation: {
      error: 'net::ERR_HTTP_RESPONSE_CODE_FAILURE',
      document: { url: URL404, status: 404, statusText: 'Not Found' },
    },
    page: { url: 'chrome-error://chromewebdata/', title: 'example.test' },
  });
  assert.deepEqual(outcome.navigationErrors, []);
  assert.equal(outcome.document.status, 404);
  assert.equal(outcome.finalUrl, URL404);
});
