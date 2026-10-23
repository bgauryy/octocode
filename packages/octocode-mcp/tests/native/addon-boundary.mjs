import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { NATIVE_ABI_VERSION } from '@octocodeai/octocode-native/runtime';

// A candidate addon to check; without one, the packaged runtime the server loads.
const [
  addon = createRequire(import.meta.url).resolve(
    '@octocodeai/octocode-native/runtime'
  ),
] = process.argv.slice(2);
const { NativeRuntime } = createRequire(import.meta.url)(addon);
const runtime = new NativeRuntime({
  surface: 'mcp',
  cwd: process.cwd(),
});
try {
  assert.equal(runtime.abiVersion, NATIVE_ABI_VERSION);
  assert.equal(runtime.closed, false);
  // A rejected call resolves as the typed tool error every surface prints.
  const rejected = await runtime.executeMcp('typed-error', 'localFetch', {
    queries: [{}],
  });
  assert.equal(rejected.isError, true);
  assert.equal(rejected.structuredContent.errorCode, 'invalidInput');
  // The envelope is the error itself: no kind/version/tool frame. The
  // first repair detail is `error`; any others follow in `details`.
  for (const frame of ['kind', 'version', 'tool']) {
    assert.equal(
      Object.hasOwn(rejected.structuredContent, frame),
      false,
      frame
    );
  }
  const repairs = [
    rejected.structuredContent.error,
    ...(rejected.structuredContent.details ?? []),
  ];
  assert.match(repairs.join('\n'), /path: Missing required field/);
  assert.deepEqual(rejected.content, [
    { type: 'text', text: JSON.stringify(rejected.structuredContent) },
  ]);
  assert.equal(runtime.cancel('missing-request'), false);
} finally {
  await runtime.close();
}
assert.equal(runtime.closed, true);
// Only a closing runtime (or a cancelled request) rejects: typed, scrubbed.
await assert.rejects(
  runtime.executeMcp('after-close', 'localFetch', { queries: [{}] }),
  error => {
    const detail = JSON.parse(error.message);
    assert.equal(detail.kind, 'octocode.nativeError');
    assert.equal(detail.code, 'closed');
    return true;
  }
);
console.log(
  JSON.stringify({ typedErrors: true, close: true, missingCancel: true })
);
