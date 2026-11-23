// A raw cdp call that names no tab, with several tabs open and none selected, runs browser-level
// methods on the browser target instead of failing as ambiguous. Skipped without Chrome.
import test from 'node:test';
import assert from 'node:assert/strict';
import { skip, withSite } from './tool-depth-fixture.mjs';

test(
  'browser-level cdp needs no tab when several are open; page methods name the fix',
  { skip, timeout: 120000 },
  async () => {
    await withSite(async ({ base, port, connection, call, ok, read }) => {
      const step = reply => reply.flow.steps[0];
      const result = row =>
        row.data?.next?.query?.file
          ? JSON.parse(read(row.data.next.query.file))
          : row.data;
      const pages = async () =>
        result(
          step(
            await ok('cdp', {
              method: 'Target.getTargets',
              scope: 'browser',
              connection,
            })
          )
        ).targetInfos.filter(info => info.type === 'page');
      // Drop the tab `open` selected, leaving two tabs and no selection.
      const [selected] = await pages();
      await ok('group', {
        action: 'open',
        port,
        name: 'pair',
        urls: [`${base}/`, `${base}/`],
        private: false,
      });
      await ok('cdp', {
        method: 'Target.closeTarget',
        params: { targetId: selected.targetId },
        scope: 'browser',
        connection,
      });
      assert.equal((await pages()).length, 2);
      const listed = (await ok('targets', { connection })).data;
      assert.equal(listed.filter(row => row.type === 'page').length, 2);
      for (const row of listed)
        assert(
          row.type === 'page' || !/^(chrome|chrome-extension):/.test(row.url),
          `targets lists Chrome's own ${row.type} ${row.url}`
        );

      for (const scope of [undefined, 'browser']) {
        const reply = await ok('cdp', {
          method: 'Browser.getVersion',
          ...(scope ? { scope } : {}),
          connection,
        });
        assert.match(result(step(reply)).product, /Chrome/);
      }

      const page = await call('cdp', {
        method: 'DOM.getDocument',
        connection,
      });
      assert.match(page.refused, /2 tabs are open and none is selected/);
      assert.match(page.refused, /connection\.target/);

      const [tab] = await pages();
      const chosen = await ok('cdp', {
        method: 'DOM.getDocument',
        params: { depth: 0 },
        connection: { port, target: tab.targetId },
      });
      assert.ok(result(step(chosen)).root, 'the chosen tab answered');
    });
  }
);
