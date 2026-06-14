import { nativeMaskSensitiveData } from './native.js';
import type { SensitiveDataPattern } from './types.js';
import { securityRegistry } from './registry.js';
import { maskEveryOtherChar } from './maskUtils.js';

// JS masking for extra/custom patterns (fileContext-free only)
function applyJsMask(
  text: string,
  patterns: readonly SensitiveDataPattern[]
): string {
  const applicable = patterns.filter(p => !p.fileContext);
  if (applicable.length === 0) return text;

  // Collect all non-overlapping match spans across all patterns
  const matches: Array<{ start: number; end: number }> = [];
  for (const p of applicable) {
    p.regex.lastIndex = 0;
    let m: RegExpExecArray | null;
    const re = new RegExp(p.regex.source, p.regex.flags.replace('g', '') + 'g');
    while ((m = re.exec(text)) !== null) {
      matches.push({ start: m.index, end: m.index + m[0].length });
      if (m[0].length === 0) re.lastIndex++;
    }
  }
  if (matches.length === 0) return text;

  // Sort by start, remove overlaps
  matches.sort((a, b) => a.start - b.start);
  const deduped: Array<{ start: number; end: number }> = [];
  let lastEnd = 0;
  for (const m of matches) {
    if (m.start >= lastEnd) {
      deduped.push(m);
      lastEnd = m.end;
    }
  }

  // Build masked string (apply in reverse to preserve indices)
  let result = text;
  for (let i = deduped.length - 1; i >= 0; i--) {
    const { start, end } = deduped[i]!;
    result =
      result.slice(0, start) +
      maskEveryOtherChar(text.slice(start, end)) +
      result.slice(end);
  }
  return result;
}

export function maskSensitiveData(
  text: string,
  patterns?: SensitiveDataPattern[]
): string {
  if (!text) return text;

  // When explicit patterns are provided, run only those in JS (bypass Rust built-ins)
  // This mirrors the TS behaviour: explicit patterns replace allRegexPatterns
  if (patterns && patterns.length > 0) {
    return applyJsMask(text, patterns);
  }

  // Run Rust engine on built-in patterns (fileContext already filtered in Rust)
  let result = nativeMaskSensitiveData(text);

  // Apply any extra patterns registered at runtime (JS fallback)
  const extra = securityRegistry.extraSecretPatterns;
  if (extra.length > 0) {
    result = applyJsMask(result, extra);
  }

  return result;
}
