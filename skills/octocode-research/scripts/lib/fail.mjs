/**
 * Source: skills/common/scripts/lib/fail.mjs; regenerate skill copies with skills/common/build.mjs.
 * Shared script error shape: compact JSON on stdout, then exit.
 * Exit 2 = invalid input, 1 = runtime failure or nothing found.
 */
export function fail(code, message, exitCode = 1, extra = {}) {
  console.log(JSON.stringify({ ok: false, ...extra, error: { code, message } }));
  process.exit(exitCode);
}

/** The message of a thrown value: Error.message, or the value as text. */
export function errorMessage(error) {
  return error instanceof Error ? error.message : String(error);
}
