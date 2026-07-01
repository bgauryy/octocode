/**
 * Backend-soundness canary fixture. soundness-canary.test.ts asserts EXACT
 * known answers against this file — do not edit without updating the test.
 * Token counts below are load-bearing:
 *   CANARY_TOKEN_A appears exactly 3 times (this comment line + two code uses).
 */
export function canaryTypedFunction(input: string): string {
  // CANARY_TOKEN_A: first code use
  return `${input}-canary`;
}

export function canaryConsumer(value: string): string {
  const viaCall = canaryTypedFunction(value);
  // CANARY_TOKEN_A: second code use
  return viaCall.toUpperCase();
}
