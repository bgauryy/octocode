import { nativeMaskSensitiveData } from './native.js';
import type { SensitiveDataPattern } from './types.js';
import { securityRegistry } from './registry.js';
import { deduplicateSpans, applyMaskToSpans, type Span } from './maskUtils.js';

function applyJsMask(
  text: string,
  patterns: readonly SensitiveDataPattern[]
): string {
  const applicable = patterns.filter(p => !p.fileContext);
  if (applicable.length === 0) return text;

  const spans: Span[] = [];
  for (const p of applicable) {
    p.regex.lastIndex = 0;
    let m: RegExpExecArray | null;
    const re = new RegExp(p.regex.source, p.regex.flags.replace('g', '') + 'g');
    while ((m = re.exec(text)) !== null) {
      spans.push({ start: m.index, end: m.index + m[0].length });
      if (m[0].length === 0) re.lastIndex++;
    }
  }
  if (spans.length === 0) return text;
  return applyMaskToSpans(text, deduplicateSpans(spans));
}

export function maskSensitiveData(text: string): string {
  if (!text) return text;

  let result = nativeMaskSensitiveData(text);

  const extra = securityRegistry.extraSecretPatterns;
  if (extra.length > 0) {
    result = applyJsMask(result, extra);
  }

  return result;
}
