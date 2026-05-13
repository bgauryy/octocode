import { allRegexPatterns } from './regexes/index.js';
import type { SensitiveDataPattern } from './regexes/types.js';
import { securityRegistry } from './registry.js';

interface Match {
  start: number;
  end: number;
  accuracy: 'high' | 'medium';
}

let combinedRegex: RegExp | null = null;
let patternMap: SensitiveDataPattern[] = [];
let cachedVersion = -1;
let cachedExplicit: SensitiveDataPattern[] | undefined;

function resolvePatterns(
  explicit?: SensitiveDataPattern[]
): SensitiveDataPattern[] {
  const base = explicit ?? allRegexPatterns;
  const extra = securityRegistry.extraSecretPatterns;
  return extra.length > 0 ? [...base, ...extra] : base;
}

/**
 * Partially mask secrets for logs — alternating characters replaced with `*`.
 *
 * @example
 * ```ts
 * maskSensitiveData('export GITHUB_TOKEN=ghp_abc123xyz');
 * // → 'export GITHUB_TOKEN=*h*_*b*1*3*y*'
 * ```
 */
export function maskSensitiveData(
  text: string,
  patterns?: SensitiveDataPattern[]
): string {
  if (!text) return text;

  const resolved = resolvePatterns(patterns);
  const regex = getCombinedRegex(resolved, patterns);
  const matches: Match[] = [];
  let match;

  while ((match = regex.exec(text)) !== null) {
    for (let i = 0; i < patternMap.length; i++) {
      if (match.groups?.[`p${i}`]) {
        matches.push({
          start: match.index,
          end: match.index + match[0].length,
          accuracy: patternMap[i]?.matchAccuracy || 'medium',
        });
        break;
      }
    }

    if (match[0].length === 0) {
      regex.lastIndex++;
    }
  }

  if (matches.length === 0) return text;

  matches.sort((a, b) => a.start - b.start);

  const nonOverlapping: Match[] = [];
  let lastEnd = -1;

  for (const match of matches) {
    if (match.start >= lastEnd) {
      nonOverlapping.push(match);
      lastEnd = match.end;
    }
  }

  let result = text;
  for (let i = nonOverlapping.length - 1; i >= 0; i--) {
    const match = nonOverlapping[i];
    if (match) {
      const originalText = text.slice(match.start, match.end);
      const maskedText = maskEveryTwoChars(originalText);

      result =
        result.slice(0, match.start) + maskedText + result.slice(match.end);
    }
  }

  return result;
}

function getCombinedRegex(
  patterns: SensitiveDataPattern[],
  explicit?: SensitiveDataPattern[]
): RegExp {
  const ver = securityRegistry.version;
  if (!combinedRegex || ver !== cachedVersion || explicit !== cachedExplicit) {
    combinedRegex = createCombinedRegex(patterns);
    patternMap = patterns;
    cachedVersion = ver;
    cachedExplicit = explicit;
  }
  combinedRegex.lastIndex = 0;
  return combinedRegex;
}

function maskEveryTwoChars(text: string): string {
  let result = '';
  for (let i = 0; i < text.length; i++) {
    if (i % 2 === 0) {
      result += '*';
    } else {
      result += text[i];
    }
  }
  return result;
}

function createCombinedRegex(patterns: SensitiveDataPattern[]): RegExp {
  const regexSources = patterns.map((pattern, index) => {
    const source = pattern.regex.source;
    return `(?<p${index}>${source})`;
  });

  return new RegExp(regexSources.join('|'), 'gi');
}
