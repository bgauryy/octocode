// Next-step help from a real isolated headless Chrome through the MCP tools: auto-enable and its
// hint, connection-override notes, plan references and waits, protocol and recipe hints, and pure
// workspace-relative `files`. Skipped without Chrome.
import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { isAbsolute, join } from 'node:path';
import { skip, withSite } from './tool-depth-fixture.mjs';
import { startThrowawayChrome } from '../fixtures/throwaway-chrome.mjs';
import {
  CONNECTION_OVERRIDE_METHODS,
  connectionOverrideNote,
} from '../../dist/engine/cdp-policy.mjs';

test('every connection-override method is a command of the installed Chrome', async () => {
  if (skip) return;
  const chrome = await startThrowawayChrome();
  try {
    const live = new Set(
      chrome.protocol.domains.flatMap(domain =>
        (domain.commands ?? []).map(c => `${domain.domain}.${c.name}`)
      )
    );
    for (const method of CONNECTION_OVERRIDE_METHODS)
      assert.ok(
        live.has(method),
        `${method} is not a command of ${chrome.version}`
      );
    assert.equal(
      new Set(CONNECTION_OVERRIDE_METHODS).size,
      CONNECTION_OVERRIDE_METHODS.length
    );
    assert.equal(connectionOverrideNote('Runtime.evaluate'), undefined);
  } finally {
    await chrome.stop();
  }
});

test('next-step help on a real Chrome', { skip, timeout: 280000 }, async t => {
  await withSite(async ({ cwd, connection, call, ok }) => {
    await t.test(
      'a method of a disabled domain auto-enables or carries the hint',
      async () => {
        for (const method of ['Accessibility.getFullAXTree', 'Log.clear']) {
          const reply = await call('cdp', { method, connection });
          assert.equal(reply.refused, undefined, `${method}: ${reply.refused}`);
        }
        const fetch = await call('cdp', {
          method: 'Fetch.getResponseBody',
          params: { requestId: 'x' },
          acknowledge: true,
          connection,
        });
        assert.match(
          fetch.refused ?? fetch.error ?? '',
          /enable:\[\\?"Fetch\\?"\]/
        );
        const after = await ok('cdp', {
          method: 'Fetch.disable',
          connection,
        });
        assert.ok(after.ok);
      }
    );

    await t.test(
      'a run that ends on a connection override warns; a dependent step in the plan does not',
      async () => {
        const single = await ok('cdp', {
          method: 'Emulation.setDeviceMetricsOverride',
          params: {
            width: 500,
            height: 400,
            deviceScaleFactor: 1,
            mobile: false,
          },
          connection,
        });
        assert.ok(
          single.warnings?.some(line =>
            line.startsWith(
              '[NOTE] Emulation.setDeviceMetricsOverride applies only to this connection'
            )
          ),
          JSON.stringify(single.warnings)
        );
        const planned = await ok('run', {
          connection,
          plan: {
            steps: [
              {
                op: 'cdp',
                method: 'Emulation.setDeviceMetricsOverride',
                params: {
                  width: 500,
                  height: 400,
                  deviceScaleFactor: 1,
                  mobile: false,
                },
              },
              { op: 'evaluate', expression: 'innerWidth' },
            ],
          },
        });
        assert.ok(!planned.warnings?.some(line => /applies only/.test(line)));
        await ok('cdp', {
          method: 'Emulation.clearDeviceMetricsOverride',
          connection,
        });
      }
    );

    await t.test(
      'a bad plan reference names the step, the pointer and the keys',
      async () => {
        const bad = await call('run', {
          connection,
          plan: {
            steps: [
              { op: 'cdp', method: 'DOM.getDocument' },
              {
                op: 'cdp',
                method: 'DOM.querySelector',
                params: {
                  nodeId: { $step: 1, pointer: '/result/root/nodeId' },
                  selector: 'h1',
                },
              },
            ],
          },
        });
        const text = bad.refused ?? JSON.stringify(bad);
        assert.match(text, /step 1, pointer \\?"\/result\/root\/nodeId\\?"/);
        assert.match(text, /top-level keys of step 1's result: root/);
        assert.match(
          text,
          /\$step\\?": N, \\?"pointer\\?": \\?"\/root\/nodeId/
        );
        const good = await ok('run', {
          connection,
          plan: {
            steps: [
              { op: 'cdp', method: 'DOM.getDocument' },
              {
                op: 'cdp',
                method: 'DOM.querySelector',
                params: {
                  nodeId: { $step: 1, pointer: '/root/nodeId' },
                  selector: 'h1',
                },
              },
            ],
          },
        });
        assert.ok(good.ok);
      }
    );

    await t.test(
      'wait takes after or value and refuses neither with a clear error',
      async () => {
        for (const wait of [
          { op: 'wait', after: { text: 'Heading' } },
          { op: 'wait', after: { selector: 'button' } },
          { op: 'wait', value: 'Heading' },
        ]) {
          const reply = await call('run', {
            connection,
            plan: { steps: [wait] },
          });
          assert.equal(
            reply.refused,
            undefined,
            `${JSON.stringify(wait)}: ${reply.refused}`
          );
          assert.ok(reply.ok);
        }
        const neither = await call('run', {
          connection,
          plan: { steps: [{ op: 'wait' }] },
        });
        assert.match(
          neither.refused ?? JSON.stringify(neither),
          /wait needs after: \{text\|selector\|url\} \(or value for text\)/
        );
      }
    );

    await t.test(
      'protocol search * is the domain index; no member hints at it',
      async () => {
        const star = await ok('protocol', { search: '*', connection });
        assert.ok(star.data.domains.length > 20);
        assert.equal(
          star.data.total,
          star.data.domains.length +
            (star.data.next ? star.data.total - star.data.domains.length : 0)
        );
        const bare = await ok('protocol', { connection });
        assert.ok(
          bare.warnings?.includes("[NOTE] use member:'*' for the domain index"),
          JSON.stringify(bare)
        );
      }
    );

    await t.test(
      'cdp-domain with no getters says so; its files are pure paths',
      async () => {
        const dump = await ok('check', {
          recipe: 'cdp-domain',
          connection,
          options: { CDPDOM_DOMAIN: 'Log', CDPDOM_MODE: 'dump' },
        });
        assert.ok(
          [...(dump.summary ?? []), ...(dump.warnings ?? [])].some(line =>
            line.startsWith(
              '[NOTE] Log has no parameterless getters; use CDPDOM_MODE=events or `protocol Log`'
            )
          ),
          JSON.stringify(dump)
        );
        for (const file of dump.files) {
          assert.ok(!isAbsolute(file) && !/\(/.test(file), file);
          assert.ok(existsSync(join(cwd, file)), file);
        }
      }
    );

    await t.test(
      'source-map on an unknown script hints at loading the page',
      async () => {
        const miss = await call('check', {
          recipe: 'source-map',
          connection,
          options: { SOURCEMAP_SCRIPT: 'nothing-like-this.js' },
        });
        const text = JSON.stringify(miss);
        assert.match(text, /SOURCEMAP_SCRIPT_NOT_FOUND/);
        assert.match(text, /load the page first/);
        assert.match(text, /check script-sources/);
      }
    );
  });
});
