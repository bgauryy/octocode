/**
 * maskUtils.ts — shared masking primitive used by mask.ts and native.ts.
 */

/**
 * Returns a copy of `text` where every even-indexed character is replaced
 * with `*`, preserving partial readability of the matched secret.
 * Mirrors the Rust implementation in detector.rs::mask_text.
 */
export function maskEveryOtherChar(text: string): string {
  let result = '';
  for (let i = 0; i < text.length; i++) {
    result += i % 2 === 0 ? '*' : text[i];
  }
  return result;
}
