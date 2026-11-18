import test from 'node:test';
import assert from 'node:assert/strict';
import { run } from '../../dist/engine/cdp-checks/browser-execute.mjs';
import { fixture } from './plan-fixture.mjs';

const entries = [
  { id: 10, url: 'https://example.test/' },
  { id: 11, url: 'https://example.test/#a' },
  { id: 12, url: 'https://example.test/next' },
];

// A page whose history sits at `current`; navigation answers with the given event.
function pageAt(cdp, emit, { current, event }) {
  let url = entries[current].url;
  cdp.send = async (method, params) => {
    cdp.calls = [...(cdp.calls ?? []), { method, params }];
    switch (method) {
      case 'Page.getFrameTree':
        return { frameTree: { frame: { id: 'main' } } };
      case 'Page.getNavigationHistory':
        return { currentIndex: current, entries };
      case 'Page.navigateToHistoryEntry': {
        url = entries.find(e => e.id === params.entryId).url;
        queueMicrotask(() => event(url));
        return {};
      }
      case 'Page.reload':
        queueMicrotask(() => event(url));
        return {};
      case 'Runtime.evaluate':
        return { result: { value: { ready: true, url, matched: true } } };
      default:
        return {};
    }
  };
}
const document = emit => url =>
  emit('Page.frameNavigated', { frame: { id: 'main', url } });
const sameDocument = emit => url =>
  emit('Page.navigatedWithinDocument', { frameId: 'main', url });

const navigated = cdp =>
  cdp.calls.filter(c => c.method === 'Page.navigateToHistoryEntry');

for (const [kind, make, direction, from, wantId, wantUrl] of [
  ['full-document', document, 'back', 2, 11, entries[1].url],
  ['same-document', sameDocument, 'back', 1, 10, entries[0].url],
  ['same-document', sameDocument, 'forward', 0, 11, entries[1].url],
])
  test(`history ${direction} (${kind}) moves one entry and reports the settled URL`, async () =>
    fixture(async ({ cdp, emit, read }) => {
      pageAt(cdp, emit, { current: from, event: make(emit) });
      await run(cdp, {
        steps: [{ op: 'history', direction, after: { url: wantUrl } }],
      });
      const result = read('browser-result.json');
      assert.equal(result.ok, true, JSON.stringify(result.failure));
      assert.deepEqual(
        navigated(cdp).map(c => c.params.entryId),
        [wantId]
      );
      assert.equal(result.steps[0].condition.url, wantUrl);
    }));

test('history at the end of the list fails without navigating', async () =>
  fixture(async ({ cdp, emit, read }) => {
    pageAt(cdp, emit, { current: 0, event: document(emit) });
    await run(cdp, { steps: [{ op: 'history', direction: 'back' }] });
    const result = read('browser-result.json');
    assert.equal(result.ok, false);
    assert.match(result.failure.error, /No back history entry/);
    assert.equal(navigated(cdp).length, 0);
  }));

test('history fails and stays uncertain when the page never navigates', async () =>
  fixture(async ({ cdp, read }) => {
    pageAt(cdp, () => {}, { current: 1, event: () => {} });
    await run(cdp, {
      steps: [{ op: 'history', direction: 'back', timeoutMs: 50 }],
    });
    const result = read('browser-result.json');
    assert.equal(result.ok, false);
    assert.match(result.failure.error, /did not navigate/);
    assert.equal(result.failure.outcomeUncertain, true);
    assert.equal(navigated(cdp).length, 1);
  }));

test('reload honors ignoreCache and waits for the postcondition', async () =>
  fixture(async ({ cdp, emit, read }) => {
    pageAt(cdp, emit, { current: 1, event: document(emit) });
    await run(cdp, {
      steps: [{ op: 'reload', ignoreCache: true, after: { selector: 'main' } }],
    });
    assert.equal(read('browser-result.json').ok, true);
    assert.deepEqual(cdp.calls.find(c => c.method === 'Page.reload').params, {
      ignoreCache: true,
    });
  }));

test('plans reject a history step without a valid direction and unknown reload fields', async () => {
  for (const step of [
    { op: 'history' },
    { op: 'history', direction: 'sideways' },
    { op: 'history', direction: 'back', url: 'https://x.test/' },
    { op: 'reload', ignoreCache: 'yes' },
    { op: 'reload', url: 'https://x.test/' },
  ])
    await fixture(async ({ cdp }) => {
      await assert.rejects(run(cdp, { steps: [step] }));
    });
});
