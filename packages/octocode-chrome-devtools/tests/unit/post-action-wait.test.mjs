import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { runInNewContext } from 'node:vm';
import { createDomOperations } from '../../dist/engine/cdp-checks/dom-operations-check.mjs';

const documentWith = text => ({
  nodeType: 9,
  body: { innerText: text },
  querySelectorAll: () => [],
});

for (const [name, connected, rootText, documentText, expected] of [
  [
    'detached control uses its frame document',
    false,
    'Continue',
    'Review ready',
    true,
  ],
  [
    'detached old text is not readiness evidence',
    false,
    'Review ready',
    'Loading',
    false,
  ],
  [
    'connected shadow control keeps its root',
    true,
    'Review ready',
    'Loading',
    true,
  ],
]) {
  test(name, async () => {
    const dir = mkdtempSync(join(tmpdir(), 'octo-post-action-'));
    const previousExitCode = process.exitCode;
    const snapshot = join(dir, 'snapshot.json');
    const resources = join(dir, 'resources.json');
    writeFileSync(
      snapshot,
      JSON.stringify({
        targetId: 'page',
        refs: { e1: { backendDOMNodeId: 10 } },
      })
    );
    writeFileSync(
      resources,
      JSON.stringify({
        resources: { 'page-snapshot:page': { artifactPath: snapshot } },
      })
    );
    const root = {
      nodeType: connected ? 11 : 1,
      textContent: rootText,
      querySelectorAll: () => [],
    };
    const target = {
      isConnected: connected,
      ownerDocument: documentWith(documentText),
      getRootNode: () => root,
    };
    const cdp = {
      outputDir: dir,
      resourcesFile: resources,
      targetInfo: { id: 'page', url: 'https://fixture/' },
      on() {},
      off() {},
      async send(method, params) {
        if (method === 'Runtime.evaluate')
          return {
            result: {
              value: { ready: 'complete', blank: false, content: true },
            },
          };
        if (method === 'DOM.resolveNode')
          return { object: { objectId: 'original-control' } };
        if (method === 'Runtime.callFunctionOn') {
          if (params.functionDeclaration.includes('const texts = []')) {
            const fn = runInNewContext(`(${params.functionDeclaration})`, {
              document: documentWith('Unrelated parent page'),
            });
            return { result: { value: fn.call(target) } };
          }
          if (params.functionDeclaration.includes('isConnected'))
            return { result: { value: true } };
          return {
            result: {
              value: { found: true, visible: true, operation: 'inspect' },
            },
          };
        }
        return {};
      },
    };
    try {
      const result = await createDomOperations({
        steps: [{ action: 'inspect', ref: 'e1' }],
        waitMs: 20,
        settleMs: 0,
        waitText: 'Review ready',
        literalWaitText: true,
        diff: false,
      }).run(cdp);
      assert.equal(result.ok, expected);
      assert.equal(result.results[0].details.postActionWait.found, expected);
    } finally {
      process.exitCode = previousExitCode;
      rmSync(dir, { recursive: true, force: true });
    }
  });
}
