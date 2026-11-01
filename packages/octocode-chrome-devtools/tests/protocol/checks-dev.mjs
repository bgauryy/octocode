// dev-check: one call reports console errors and warnings with source-mapped stacks,
// failed and 4xx requests, and a screenshot, from the isolated suite browser.
import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { suite, cli, checkList } from './harness.mjs';

const { checks, define } = checkList();
export { checks };

const devCheck = async extra => {
  const r = await cli(
    [
      'dev-check',
      '--url',
      suite.base + '/dev-check',
      ...extra,
      ...(suite.transport === 'legacy' ? ['--port', suite.port] : []),
    ],
    90000
  );
  assert.equal(r.code, 0, r.out);
  return JSON.parse(r.out.split('\n').find(line => line.startsWith('{')));
};

define('dev-check console, network and screenshot evidence', [
  'Debugger.enable',
  'Page.captureScreenshot',
], async () => {
  const report = await devCheck(['--wait-until', 'networkIdle']);
  assert.equal(report.settled, true, JSON.stringify(report));
  assert.equal(report.clean, false);
  assert.equal(report.browser.isolated, true);
  assert.ok(existsSync(report.screenshot), report.screenshot);
  assert.ok(existsSync(report.report));
  assert.ok(
    report.warnings.some(row => row.message.includes('dev warning')),
    JSON.stringify(report.warnings)
  );
  const thrown = report.errors.find(row => row.kind === 'exception');
  assert.match(thrown.message, /dev boom/);
  assert.deepEqual(
    { ...thrown.stack[0].original, column: undefined },
    { source: 'src/app.ts', line: 3, column: undefined, name: null }
  );
  const logged = report.errors.find(
    row => row.kind === 'console' && row.message.includes('dev boot failed')
  );
  assert.equal(logged.stack[0].original.line, 2);
  assert.ok(
    report.failedRequests.some(
      row => row.url.endsWith('/dev-check/missing') && row.status === 404
    ),
    JSON.stringify(report.failedRequests)
  );
  assert.ok(
    report.failedRequests.some(
      row => row.url.includes(':9/refused') && row.errorText
    ),
    JSON.stringify(report.failedRequests)
  );
});

define('dev-check load wait with a zero-second ceiling still reports', [
  'Page.captureScreenshot',
], async () => {
  const report = await devCheck(['--timeout-ms', '1', '--full-page']);
  assert.equal(typeof report.settled, 'boolean');
  assert.ok(existsSync(report.screenshot), report.screenshot);
});
