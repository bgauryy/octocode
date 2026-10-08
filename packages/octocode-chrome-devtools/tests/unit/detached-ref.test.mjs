import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createDomOperations } from '../../dist/engine/cdp-checks/dom-operations-check.mjs';
async function fixture(mode, fn) {
  const dir = mkdtempSync(join(tmpdir(), 'octo-detached-')),
    sent = [],
    previous = process.exitCode;
  const snapshot = join(dir, 'snapshot.json'),
    resources = join(dir, 'resources.json');
  writeFileSync(
    snapshot,
    JSON.stringify({
      targetId: 'page',
      refs: { e1: { backendDOMNodeId: 10, role: 'button', name: 'Advance' } },
    })
  );
  writeFileSync(
    resources,
    JSON.stringify({
      resources: { 'page-snapshot': { artifactPath: snapshot } },
    })
  );
  const cdp = {
    outputDir: dir,
    resourcesFile: resources,
    targetInfo: { id: 'page', url: 'https://fixture/' },
    on() {},
    off() {},
    foreground: async () => {},
    send: async (method, params) => {
      sent.push({ method, params });
      if (method === 'Runtime.evaluate')
        return {
          result: { value: { ready: 'complete', blank: false, content: true } },
        };
      if (method === 'DOM.resolveNode')
        return {
          object: {
            objectId: params.backendNodeId === 10 ? 'detached' : 'replacement',
          },
        };
      if (method === 'Runtime.callFunctionOn') {
        if (params.functionDeclaration.includes('isConnected')) {
          if (mode === 'error')
            throw new Error('Connectivity request cancelled');
          return { result: { value: params.objectId === 'replacement' } };
        }
        return {
          result: {
            value:
              params.objectId === 'replacement'
                ? { found: true, visible: true, operation: 'inspect' }
                : {
                    found: false,
                    operation: 'blocked-by-actionability',
                    error: 'Detached original target',
                  },
          },
        };
      }
      if (method === 'Page.getFrameTree')
        return {
          frameTree: { frame: { id: 'main', url: 'https://fixture/' } },
        };
      if (method === 'Accessibility.getFullAXTree')
        return {
          nodes:
            mode === 'unique'
              ? [
                  {
                    nodeId: 'root',
                    role: { value: 'RootWebArea' },
                    name: { value: 'Fixture' },
                    childIds: ['button'],
                  },
                  {
                    nodeId: 'button',
                    parentId: 'root',
                    role: { value: 'button' },
                    name: { value: 'Advance' },
                    backendDOMNodeId: 20,
                    properties: [],
                  },
                ]
              : [],
        };
      if (method === 'DOMSnapshot.captureSnapshot')
        return { strings: [], documents: [] };
      return {};
    },
  };
  try {
    await fn({ cdp, sent, dir });
  } finally {
    process.exitCode = previous;
    rmSync(dir, { recursive: true, force: true });
  }
}
test('detached resolved ref recovers before input or refuses with exact stale diagnostic', async () => {
  for (const mode of ['none', 'unique'])
    await fixture(mode, async ({ cdp, sent, dir }) => {
      const result = await createDomOperations({
        steps: [{ action: mode === 'none' ? 'click' : 'inspect', ref: 'e1' }],
        waitMs: 1,
        settleMs: 0,
        diff: false,
      }).run(cdp);
      const details = JSON.parse(
        readFileSync(join(dir, 'dom-check.json'), 'utf8')
      );
      if (mode === 'none') {
        assert.equal(result.ok, false);
        assert.match(
          details.error,
          /Stale ref recovery found 0 role\/name matches/
        );
      } else {
        assert.equal(result.ok, true);
        assert.equal(details.recoveredFromStaleRef, true);
        assert.equal(result.results[0].label, 'ref:e1');
      }
      assert.ok(!sent.some(row => row.method.startsWith('Input.')));
      assert.ok(
        sent.some(
          row =>
            row.method === 'Runtime.callFunctionOn' &&
            row.params.objectId === 'detached' &&
            row.params.functionDeclaration.includes('isConnected')
        )
      );
    });
});
test('connectivity failure propagates without treating cancellation as stale recovery', async () =>
  fixture('error', async ({ cdp, sent }) => {
    await assert.rejects(
      createDomOperations({
        steps: [{ action: 'click', ref: 'e1' }],
        waitMs: 1,
        diff: false,
      }).run(cdp),
      /Connectivity request cancelled/
    );
    assert.ok(
      !sent.some(
        row =>
          row.method.startsWith('Input.') ||
          row.method === 'Accessibility.getFullAXTree'
      )
    );
  }));
