import { nativeSanitizeContent } from './native.js';
import type { SensitiveDataPattern } from './types.js';
import type { ISanitizer, SanitizationResult, ValidationResult } from './types.js';
import { securityRegistry } from './registry.js';

const MAX_STRING_LENGTH = 10_000;
const MAX_STRING_LENGTH_DISPLAY = '10,000';
const MAX_ARRAY_LENGTH = 100;
const MAX_DEPTH = 20;
const DANGEROUS_KEYS = new Set(['__proto__', 'constructor', 'prototype']);

// ---------------------------------------------------------------------------
// Extra-pattern JS fallback (for patterns added via securityRegistry at runtime)
// ---------------------------------------------------------------------------
function detectWithExtraPatterns(
  content: string,
  filePath: string | undefined,
  extraPatterns: readonly SensitiveDataPattern[]
): { sanitized: string; secrets: string[] } {
  let sanitized = content;
  const secrets: string[] = [];
  for (const pattern of extraPatterns) {
    if (
      pattern.fileContext &&
      (!filePath || !pattern.fileContext.test(filePath))
    ) {
      continue;
    }
    // Reset lastIndex for global regexes
    pattern.regex.lastIndex = 0;
    if (pattern.regex.test(sanitized)) {
      secrets.push(pattern.name);
      pattern.regex.lastIndex = 0;
      sanitized = sanitized.replace(
        pattern.regex,
        `[REDACTED-${pattern.name.toUpperCase()}]`
      );
    }
    pattern.regex.lastIndex = 0;
  }
  return { sanitized, secrets };
}

// ---------------------------------------------------------------------------
// Pure-JS detection for custom / extra patterns
// ---------------------------------------------------------------------------
function jsDetectSecrets(
  content: string,
  filePath: string | undefined,
  patterns: readonly SensitiveDataPattern[]
): { sanitized: string; secrets: string[] } {
  const MAX_CONTENT_SIZE = 10_000_000;
  const CHUNK_SIZE = 500_000;
  const CHUNK_OVERLAP = 1_000;

  if (content.length > MAX_CONTENT_SIZE) {
    return {
      sanitized: '[CONTENT-REDACTED-SIZE-LIMIT]',
      secrets: ['content-size-exceeded'],
    };
  }

  const applicable = patterns.filter(
    p => !p.fileContext || (filePath && p.fileContext.test(filePath))
  );

  try {
    if (content.length <= CHUNK_SIZE) {
      // single-pass
      let sanitized = content;
      const secrets: string[] = [];
      for (const p of applicable) {
        p.regex.lastIndex = 0;
        if (p.regex.test(sanitized)) {
          secrets.push(p.name);
          p.regex.lastIndex = 0;
          sanitized = sanitized.replace(
            p.regex,
            `[REDACTED-${p.name.toUpperCase()}]`
          );
        }
        p.regex.lastIndex = 0;
      }
      return { sanitized, secrets };
    }

    // chunked
    let sanitized = content;
    const secretSet = new Set<string>();
    for (const p of applicable) {
      let chunkStart = 0;
      while (chunkStart < sanitized.length) {
        const chunkEnd = Math.min(chunkStart + CHUNK_SIZE, sanitized.length);
        const chunk = sanitized.slice(chunkStart, chunkEnd);
        p.regex.lastIndex = 0;
        if (p.regex.test(chunk)) {
          secretSet.add(p.name);
          p.regex.lastIndex = 0;
          const replacement = `[REDACTED-${p.name.toUpperCase()}]`;
          const newChunk = chunk.replace(p.regex, replacement);
          sanitized =
            sanitized.slice(0, chunkStart) +
            newChunk +
            sanitized.slice(chunkEnd);
        }
        p.regex.lastIndex = 0;
        const next = chunkEnd - CHUNK_OVERLAP;
        if (next <= chunkStart) break;
        chunkStart = next;
      }
    }
    return { sanitized, secrets: Array.from(secretSet) };
  } catch {
    return {
      sanitized: '[CONTENT-REDACTED-DETECTION-ERROR]',
      secrets: ['detection-error'],
    };
  }
}

export const ContentSanitizer: ISanitizer = {
  sanitizeContent(
    content: string,
    filePath?: string,
    patterns?: SensitiveDataPattern[]
  ): SanitizationResult {
    if (content == null || typeof content !== 'string') {
      return {
        content: content == null ? '' : String(content),
        hasSecrets: false,
        secretsDetected: [],
        warnings: [],
      };
    }

    // Explicit patterns: run in pure JS (matches original TS behaviour)
    if (patterns && patterns.length > 0) {
      const { sanitized, secrets } = jsDetectSecrets(
        content,
        filePath,
        patterns
      );
      const hasSecrets = secrets.length > 0;
      return {
        content: sanitized,
        hasSecrets,
        secretsDetected: secrets,
        warnings: hasSecrets ? [`${secrets.length} secret(s) redacted`] : [],
      };
    }

    // --- Rust fast path (built-in patterns) ---
    const rustResult = nativeSanitizeContent(content, filePath ?? null);

    // --- JS fallback for any extra patterns added via registry ---
    const extraPatterns = securityRegistry.extraSecretPatterns;
    if (extraPatterns.length > 0) {
      const { sanitized: finalContent, secrets: extraSecrets } =
        detectWithExtraPatterns(rustResult.content, filePath, extraPatterns);

      const allSecrets = [...rustResult.secretsDetected, ...extraSecrets];
      const hasSecrets = allSecrets.length > 0;
      return {
        content: finalContent,
        hasSecrets,
        secretsDetected: allSecrets,
        warnings: hasSecrets ? [`${allSecrets.length} secret(s) redacted`] : [],
      };
    }

    return {
      content: rustResult.content,
      hasSecrets: rustResult.hasSecrets,
      secretsDetected: rustResult.secretsDetected,
      warnings: rustResult.warnings,
    };
  },

  validateInputParameters(
    params: Record<string, unknown>
  ): ValidationResult {
    return validateRecursive(params, 0, new WeakSet<object>());
  },
};

// ---------------------------------------------------------------------------
// Pure-TS recursive validation (identical logic to former octocode-security-utils)
// ---------------------------------------------------------------------------
function validateRecursive(
  params: Record<string, unknown>,
  depth: number,
  ancestorStack: WeakSet<object>
): ValidationResult {
  if (!params || typeof params !== 'object') {
    return {
      sanitizedParams: {},
      isValid: false,
      hasSecrets: false,
      warnings: ['Invalid parameters: must be an object'],
    };
  }
  if (depth > MAX_DEPTH) {
    return {
      sanitizedParams: {},
      isValid: false,
      hasSecrets: false,
      warnings: ['Maximum nesting depth exceeded'],
    };
  }
  // `ancestorStack` tracks only the CURRENT recursion path (entries removed on
  // exit), so a DAG — the same object appearing under two sibling keys — is
  // legal while a true cycle is still caught.
  if (ancestorStack.has(params)) {
    return {
      sanitizedParams: {},
      isValid: false,
      hasSecrets: false,
      warnings: ['Circular reference detected'],
    };
  }
  ancestorStack.add(params);

  const sanitizedParams: Record<string, unknown> = {};
  const warnings = new Set<string>();
  let hasSecrets = false;
  let hasValidationErrors = false;

  for (const [key, value] of Object.entries(params)) {
    if (typeof key !== 'string' || key.trim() === '') {
      warnings.add(`Invalid parameter key: ${key}`);
      hasValidationErrors = true;
      continue;
    }
    if (DANGEROUS_KEYS.has(key)) {
      warnings.add(`Dangerous parameter key blocked: ${key}`);
      hasValidationErrors = true;
      continue;
    }

    if (typeof value === 'string') {
      let v = value;
      if (v.length > MAX_STRING_LENGTH) {
        warnings.add(
          `Parameter ${key} exceeds maximum length (${MAX_STRING_LENGTH_DISPLAY} characters)`
        );
        v = v.substring(0, MAX_STRING_LENGTH);
      }
      const r = ContentSanitizer.sanitizeContent(v, undefined);
      if (r.hasSecrets) {
        hasSecrets = true;
        r.secretsDetected.forEach((s: string) =>
          warnings.add(`Secrets detected in ${key}: ${s}`)
        );
      }
      sanitizedParams[key] = r.content;
    } else if (Array.isArray(value)) {
      const truncated =
        value.length > MAX_ARRAY_LENGTH
          ? (() => {
              warnings.add(
                `Parameter ${key} array exceeds maximum length (${MAX_ARRAY_LENGTH} items)`
              );
              return value.slice(0, MAX_ARRAY_LENGTH);
            })()
          : value;

      let arrHasSecrets = false;
      let arrHasErrors = false;
      const sanitizedArr = truncated.map(item => {
        if (typeof item === 'string') {
          const r = ContentSanitizer.sanitizeContent(item, undefined);
          if (r.hasSecrets) {
            arrHasSecrets = true;
          }
          return r.content;
        }
        if (item !== null && typeof item === 'object' && !Array.isArray(item)) {
          const r = validateRecursive(
            item as Record<string, unknown>,
            depth + 1,
            ancestorStack
          );
          if (r.hasSecrets) arrHasSecrets = true;
          if (!r.isValid) {
            arrHasErrors = true;
            r.warnings.forEach(w => warnings.add(`${key}[]: ${w}`));
          }
          return r.sanitizedParams;
        }
        return item;
      });
      if (arrHasSecrets) hasSecrets = true;
      if (arrHasErrors) hasValidationErrors = true;
      sanitizedParams[key] = sanitizedArr;
    } else if (value !== null && typeof value === 'object') {
      const r = validateRecursive(
        value as Record<string, unknown>,
        depth + 1,
        ancestorStack
      );
      if (r.hasSecrets) hasSecrets = true;
      if (!r.isValid) {
        hasValidationErrors = true;
        r.warnings.forEach(w =>
          warnings.add(`Invalid nested object in parameter ${key}: ${w}`)
        );
      } else {
        sanitizedParams[key] = r.sanitizedParams;
      }
    } else {
      sanitizedParams[key] = value;
    }
  }

  ancestorStack.delete(params);

  return {
    sanitizedParams,
    isValid: !hasValidationErrors,
    hasSecrets,
    warnings: Array.from(warnings),
  };
}
