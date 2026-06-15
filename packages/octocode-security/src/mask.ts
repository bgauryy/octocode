import { nativeMaskSensitiveData } from './native.js';
import type { SensitiveDataPattern } from './types.js';
import { securityRegistry } from './registry.js';
import { maskEveryOtherChar } from './maskUtils.js';

function applyJsMask(
  text: string,
  patterns: readonly SensitiveDataPattern[]
): string {
  const applicable = patterns.filter(p => !p.fileContext);
  if (applicable.length === 0) return text;

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

  matches.sort((a, b) => a.start - b.start);
  const deduped: Array<{ start: number; end: number }> = [];
  let lastEnd = 0;
  for (const m of matches) {
    if (m.start >= lastEnd) {
      deduped.push(m);
      lastEnd = m.end;
    }
  }

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

export function maskSensitiveData(text: string): string {
  if (!text) return text;

  let result = nativeMaskSensitiveData(text);

  const extra = securityRegistry.extraSecretPatterns;
  if (extra.length > 0) {
    result = applyJsMask(result, extra);
  }

  return result;
}
