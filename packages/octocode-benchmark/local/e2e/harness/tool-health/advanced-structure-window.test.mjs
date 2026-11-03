import assert from 'node:assert/strict';
import test from 'node:test';
import { inspectStructureWindow } from './advanced-structure-window.mjs';

const response = data => ({ result: { json: { results: [{ status: 'success', data }] } } });
const file = path => ({ path });

test('scan-window oracle requires canonical continuation and preserves multiplicity', () => {
  const first = response({ files: [file('/a')], partialReasons: ['walkWindow'], next: { nextPage: { input: { cursor: 'next' } } } });
  const complete = inspectStructureWindow([first, response({ files: [file('/b')] })], ['/a', '/b']);
  assert.deepEqual(complete, { errors: 0, windowLeads: 1, wrongKeys: [], missing: [], extra: [], duplicates: 0, actual: 2, expected: 2 });
  const repeated = inspectStructureWindow([first, first], ['/a', '/b']);
  assert.equal(repeated.duplicates, 1);
  assert.deepEqual(repeated.missing, ['/b']);
  const obsolete = inspectStructureWindow([response({ files: [file('/a')], partialReasons: ['walkWindow'], next: { continueWalk: {} } })], ['/a']);
  assert.equal(obsolete.windowLeads, 0);
  assert.deepEqual(obsolete.wrongKeys, ['continueWalk']);
});

test('scan-window oracle exposes errors after complete-looking evidence', () => {
  const responses = [response({ files: [file('/a')] }), response({ errorCode: 'executionFailed', error: 'later window rejected' })];
  const actual = inspectStructureWindow(responses, ['/a']);
  assert.equal(actual.actual, actual.expected);
  assert.equal(actual.errors, 1);
});
