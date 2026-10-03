/**
 * Test double for `NativeRuntime.normalizeInput` in suites that run without the
 * native addon: only the bare-query envelope wrapping native applies. The
 * lossless repairs are native-owned and asserted through the real addon in
 * native-normalize.test.ts.
 */
export const wrapBareQuery = (input: unknown): unknown =>
  input &&
  typeof input === 'object' &&
  !Array.isArray(input) &&
  !('queries' in input) &&
  Object.keys(input).length > 0
    ? { queries: [input] }
    : input;
