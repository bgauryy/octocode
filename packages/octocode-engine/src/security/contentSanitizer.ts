import { nativeSanitizeContent } from './native.js';
import type { SensitiveDataPattern } from './types.js';
import type {
  ISanitizer,
  SanitizationResult,
  ValidationResult,
} from './types.js';
import { securityRegistry } from './registry.js';

const MAX_STRING_LENGTH = 10_000;
const MAX_STRING_LENGTH_DISPLAY = '10,000';
const MAX_ARRAY_LENGTH = 100;
const MAX_DEPTH = 20;
const DANGEROUS_KEYS = new Set(['__proto__', 'constructor', 'prototype']);

function shouldApplyExtraPattern(
  fileContext: RegExp | undefined,
  filePath: string | undefined
): boolean {
  if (!fileContext) return true;
  if (!filePath) return false;

  fileContext.lastIndex = 0;
  const applies = fileContext.test(filePath);
  fileContext.lastIndex = 0;
  return applies;
}

function detectWithExtraPatterns(
  content: string,
  filePath: string | undefined,
  extraPatterns: readonly SensitiveDataPattern[]
): { sanitized: string; secrets: string[] } {
  let sanitized = content;
  const secrets: string[] = [];
  for (const pattern of extraPatterns) {
    if (!shouldApplyExtraPattern(pattern.fileContext, filePath)) {
      continue;
    }
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

export const ContentSanitizer: ISanitizer = {
  sanitizeContent(content: string, filePath?: string): SanitizationResult {
    if (content == null || typeof content !== 'string') {
      return {
        content: content == null ? '' : String(content),
        hasSecrets: false,
        secretsDetected: [],
        warnings: [],
      };
    }

    const rustResult = nativeSanitizeContent(content, filePath ?? null);

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

  validateInputParameters(params: Record<string, unknown>): ValidationResult {
    return validateRecursive(params, 0, new WeakSet<object>());
  },
};

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
      }
      // Always include sanitized data — tool handlers must not receive undefined
      // for a key that was present in the input, even when the nested object had
      // validation issues. The sanitized partial result is still safer than the raw.
      sanitizedParams[key] = r.sanitizedParams;
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
