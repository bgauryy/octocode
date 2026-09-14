import assert from 'node:assert/strict';
import { createRequire } from 'node:module';

const [addon, regexWorker] = process.argv.slice(2);
assert.ok(addon && regexWorker, 'usage: node addon-boundary.mjs <addon> <regex-worker>');
const binding = createRequire(import.meta.url)(addon);
const {
  NativeRuntime,
  nativeAbiVersion,
  storeCredentials,
  getCredentials,
  deleteCredentials,
  refreshAuthToken,
  getTokenWithRefresh,
} = binding;
assert.equal(typeof nativeAbiVersion, 'function');
assert.equal(nativeAbiVersion(), 2);
assert.equal(typeof storeCredentials, 'function');
assert.equal(typeof getCredentials, 'function');
assert.equal(typeof deleteCredentials, 'function');
assert.equal(typeof refreshAuthToken, 'function');
assert.equal(typeof getTokenWithRefresh, 'function');
const runtime = new NativeRuntime({
  surface: 'mcp',
  regexWorkerPath: regexWorker,
  cwd: process.cwd(),
});
try {
  assert.equal(runtime.abiVersion, 2);
  assert.equal(typeof runtime.storeCredentials, 'function');
  assert.equal(typeof runtime.getCredentials, 'function');
  assert.equal(typeof runtime.deleteCredentials, 'function');
  assert.equal(typeof runtime.refreshAuthToken, 'function');
  assert.equal(typeof runtime.getTokenWithRefresh, 'function');
  assert.equal(runtime.closed, false);
  await assert.rejects(
    runtime.execute('typed-error', 'localFetch', { queries: [{}] }),
    error => {
      const detail = JSON.parse(error.message);
      assert.equal(detail.kind, 'octocode.nativeError');
      assert.equal(detail.code, 'invalidInput');
      assert.equal(detail.payload.kind, 'octocode.toolError');
      assert.match(detail.payload.details.join('\n'), /queries\.0\.path/);
      return true;
    },
  );
  assert.equal(runtime.cancel('missing-request'), false);
} finally {
  await runtime.close();
}
assert.equal(runtime.closed, true);
console.log(JSON.stringify({ typedErrors: true, close: true, missingCancel: true }));
