// coverage against a real headless Chrome: a known unused function and CSS rule are reported
// with used + unused == total, and each unused range points at the unused text.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fixtureFiles, skip, startHarness } from './coverage-fixture.mjs';

const { usedJsRanges, complement, mergeRanges } = await import(
  join(import.meta.dirname, '../../dist/engine/cdp-checks/coverage-check.mjs')
);

test('nested block coverage: the innermost range decides, and used + unused covers the text', () => {
  const used = usedJsRanges([
    { startOffset: 0, endOffset: 100, count: 1 },
    { startOffset: 20, endOffset: 40, count: 0 },
    { startOffset: 25, endOffset: 30, count: 2 },
  ]);
  assert.deepEqual(used, [
    { start: 0, end: 20 },
    { start: 25, end: 30 },
    { start: 40, end: 100 },
  ]);
  const unused = complement(used, 100);
  assert.deepEqual(unused, [
    { start: 20, end: 25 },
    { start: 30, end: 40 },
  ]);
  const bytes = rs => rs.reduce((n, r) => n + r.end - r.start, 0);
  assert.equal(bytes(used) + bytes(unused), 100);
  assert.deepEqual(
    mergeRanges([
      { start: 5, end: 9 },
      { start: 1, end: 6 },
    ]),
    [{ start: 1, end: 9 }]
  );
});

test(
  'coverage reports the unused function and CSS rule with consistent totals',
  { skip, timeout: 180000 },
  async () => {
    const h = await startHarness();
    try {
      assert.equal(h.opened.ok, true);
      const run = await h.check('coverage', { COVERAGE_URL: `${h.origin}/` });
      assert.equal(run.out.ok, true, run.stdout);
      const report = JSON.parse(
        readFileSync(run.artifacts['coverage.json'], 'utf8')
      );
      const ranges = JSON.parse(
        readFileSync(run.artifacts['coverage-ranges.json'], 'utf8')
      ).ranges;

      for (const row of report.entries)
        assert.equal(row.usedBytes + row.unusedBytes, row.totalBytes, row.url);

      for (const [path, type, marker] of [
        ['/app.js', 'js', 'function unusedFn'],
        ['/app.css', 'css', '.never-matched'],
      ]) {
        const text = fixtureFiles[path][1];
        const row = report.entries.find(
          e => e.url === `${h.origin}${path}` && e.type === type
        );
        assert.ok(row, `${path} measured`);
        assert.equal(row.totalBytes, text.length);
        assert.ok(row.unusedBytes > 0 && row.percentUsed < 100);
        const at = text.indexOf(marker);
        assert.ok(
          ranges.some(
            r =>
              r.url === row.url &&
              r.type === type &&
              r.start <= at &&
              at < r.end
          ),
          `${path} unused range covers ${marker}`
        );
        // The used half is not reported unused.
        const usedAt = text.indexOf(
          type === 'js' ? 'function usedFn' : '.used'
        );
        assert.ok(
          !ranges.some(
            r => r.url === row.url && r.start <= usedAt && usedAt < r.end
          )
        );
      }
      assert.match(run.stdout, /\[METRIC\] COVERAGE js /);
      assert.ok(!run.stdout.includes('function unusedFn'), 'source not echoed');

      // JS only: no CSS entries.
      const jsOnly = await h.check('coverage', {
        COVERAGE_URL: `${h.origin}/`,
        COVERAGE_CSS: false,
      });
      const jsReport = JSON.parse(
        readFileSync(jsOnly.artifacts['coverage.json'], 'utf8')
      );
      assert.ok(
        jsReport.entries.length && jsReport.entries.every(e => e.type === 'js')
      );
    } finally {
      await h.stop();
    }
  }
);
