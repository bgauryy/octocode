// trace-capture against a real headless Chrome: trace.json is Chrome trace JSON with events.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { skip, startHarness } from './coverage-fixture.mjs';

test(
  'trace-capture writes loadable trace JSON with events, and requires TRACE_MS',
  { skip, timeout: 180000 },
  async () => {
    const h = await startHarness();
    try {
      assert.equal(h.opened.ok, true);
      const run = await h.check('trace-capture', {
        TRACE_URL: `${h.origin}/`,
        TRACE_MS: 800,
      });
      assert.equal(run.out.ok, true, run.stdout);
      const file = run.artifacts.TRACE;
      assert.ok(file, run.stdout);
      const bytes = readFileSync(file);
      const trace = JSON.parse(bytes.toString('utf8'));
      assert.ok(
        Array.isArray(trace.traceEvents) && trace.traceEvents.length > 0
      );
      assert.match(run.stdout, new RegExp(`bytes=${bytes.length} `));

      const custom = await h.check('trace-capture', {
        TRACE_MS: 200,
        TRACE_CATEGORIES: ['devtools.timeline', 'blink.user_timing'],
      });
      assert.equal(custom.out.ok, true, custom.stdout);
      const events = JSON.parse(
        readFileSync(custom.artifacts.TRACE, 'utf8')
      ).traceEvents;
      assert.ok(Array.isArray(events));

      const missing = await h.check('trace-capture', {});
      assert.notEqual(missing.out.ok, true);
    } finally {
      await h.stop();
    }
  }
);
