// A check reply's summary is bounded by lines and bytes; the overflow is flagged, never dropped silently.
import test from 'node:test';
import assert from 'node:assert/strict';
import {
  REPLY_SUMMARY_BYTES,
  REPLY_SUMMARY_LINES,
  replySummary,
} from '../../dist/engine/capture-result.mjs';

test('summary keeps whole lines within the line and byte ceilings and flags the rest', () => {
  const few = ['[METRIC] A=1', '[FINDING] B_C x'];
  assert.deepEqual(replySummary(few), { lines: few, truncated: false });
  const many = Array.from(
    { length: REPLY_SUMMARY_LINES + 3 },
    (_, i) => `[METRIC] n=${i}`
  );
  const byLines = replySummary(many);
  assert.equal(byLines.lines.length, REPLY_SUMMARY_LINES);
  assert.equal(byLines.truncated, true);
  const wide = Array.from(
    { length: 5 },
    (_, i) => `[METRIC] ${i} ${'x'.repeat(600)}`
  );
  const byBytes = replySummary(wide);
  assert(Buffer.byteLength(byBytes.lines.join('\n')) <= REPLY_SUMMARY_BYTES);
  assert(byBytes.lines.length < wide.length && byBytes.truncated);
  assert.deepEqual(byBytes.lines, wide.slice(0, byBytes.lines.length));
});
