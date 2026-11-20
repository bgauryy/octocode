// heap-snapshot against a real headless Chrome: the file is a snapshot JSON of the reported size.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, statSync } from 'node:fs';
import { skip, startHarness } from './coverage-fixture.mjs';

test(
  'heap-snapshot streams a parseable .heapsnapshot whose size matches the reported bytes',
  { skip, timeout: 180000 },
  async () => {
    const h = await startHarness();
    try {
      assert.equal(h.opened.ok, true);
      for (const collect of [false, true]) {
        const run = await h.check('heap-snapshot', {
          HEAP_COLLECT_GARBAGE: collect,
        });
        assert.equal(run.out.ok, true, run.stdout);
        const file = run.artifacts.HEAP_SNAPSHOT;
        assert.match(file, /\.heapsnapshot$/);
        const size = statSync(file).size;
        assert.match(run.stdout, new RegExp(`bytes=${size} `));
        const text = readFileSync(file, 'utf8');
        assert.ok(text.startsWith('{"snapshot":'), text.slice(0, 40));
        const snapshot = JSON.parse(text);
        assert.ok(snapshot.snapshot.meta && Array.isArray(snapshot.nodes));
        assert.ok(snapshot.nodes.length > 0 && Array.isArray(snapshot.strings));
      }
    } finally {
      await h.stop();
    }
  }
);
