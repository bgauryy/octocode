import { allRegexPatterns } from './regexes/index.js';
import type { SensitiveDataPattern } from './regexes/types.js';
import type { SanitizationResult, ValidationResult } from './types.js';
import { securityRegistry } from './registry.js';

let _cachedPatterns: SensitiveDataPattern[] | null = null;
let _cachedVersion = -1;

function getAllPatterns(
  explicit?: SensitiveDataPattern[]
): SensitiveDataPattern[] {
  if (explicit) {
    const extra = securityRegistry.extraSecretPatterns;
    return extra.length > 0 ? [...explicit, ...extra] : explicit;
  }
  const ver = securityRegistry.version;
  if (_cachedPatterns && ver === _cachedVersion) return _cachedPatterns;
  const extra = securityRegistry.extraSecretPatterns;
  _cachedPatterns =
    extra.length > 0 ? [...allRegexPatterns, ...extra] : allRegexPatterns;
  _cachedVersion = ver;
  return _cachedPatterns;
}

export class ContentSanitizer {
  /** Sanitize a single string value, enforcing max length and scanning for secrets. */
  private static sanitizeStringValue(
    key: string,
    value: string,
    warnings: Set<string>
  ): { sanitized: string; hasSecrets: boolean } {
    let sanitizedValue = value;
    if (value.length > 10000) {
      warnings.add(
        `Parameter ${key} exceeds maximum length (10,000 characters)`
      );
      sanitizedValue = value.substring(0, 10000);
    }
    const secretsResult = this.detectSecrets(sanitizedValue);
    if (secretsResult.hasSecrets) {
      secretsResult.secretsDetected.forEach(secret =>
        warnings.add(`Secrets detected in ${key}: ${secret}`)
      );
    }
    return {
      sanitized: secretsResult.sanitizedContent,
      hasSecrets: secretsResult.hasSecrets,
    };
  }

  /** Sanitize a single array item (string or nested object). */
  private static sanitizeArrayItem(
    key: string,
    item: unknown,
    depth: number,
    visited: WeakSet<object>,
    warnings: Set<string>
  ): { sanitized: unknown; hasSecrets: boolean; hasValidationErrors: boolean } {
    if (typeof item === 'string') {
      const secretsResult = this.detectSecrets(item);
      if (secretsResult.hasSecrets) {
        secretsResult.secretsDetected.forEach(secret =>
          warnings.add(`Secrets detected in ${key}[]: ${secret}`)
        );
      }
      return {
        sanitized: secretsResult.sanitizedContent,
        hasSecrets: secretsResult.hasSecrets,
        hasValidationErrors: false,
      };
    }
    if (item !== null && typeof item === 'object' && !Array.isArray(item)) {
      const nestedValidation = this.validateRecursive(
        item as Record<string, unknown>,
        depth + 1,
        visited
      );
      if (!nestedValidation.isValid) {
        nestedValidation.warnings.forEach(w => warnings.add(`${key}[]: ${w}`));
      }
      return {
        sanitized: nestedValidation.sanitizedParams,
        hasSecrets: nestedValidation.hasSecrets,
        hasValidationErrors: !nestedValidation.isValid,
      };
    }
    return { sanitized: item, hasSecrets: false, hasValidationErrors: false };
  }

  /** Sanitize an array parameter, enforcing max item count. */
  private static sanitizeArrayValue(
    key: string,
    value: unknown[],
    depth: number,
    visited: WeakSet<object>,
    warnings: Set<string>
  ): {
    sanitized: unknown[];
    hasSecrets: boolean;
    hasValidationErrors: boolean;
  } {
    const truncated =
      value.length > 100
        ? (warnings.add(
            `Parameter ${key} array exceeds maximum length (100 items)`
          ),
          value.slice(0, 100))
        : value;
    let hasSecrets = false;
    let hasValidationErrors = false;
    const sanitized = truncated.map(item => {
      const result = this.sanitizeArrayItem(
        key,
        item,
        depth,
        visited,
        warnings
      );
      if (result.hasSecrets) hasSecrets = true;
      if (result.hasValidationErrors) hasValidationErrors = true;
      return result.sanitized;
    });
    return { sanitized, hasSecrets, hasValidationErrors };
  }

  /** Sanitize a nested object parameter. */
  private static sanitizeNestedObject(
    key: string,
    value: Record<string, unknown>,
    depth: number,
    visited: WeakSet<object>,
    warnings: Set<string>
  ): {
    sanitized: Record<string, unknown> | null;
    hasSecrets: boolean;
    isValid: boolean;
  } {
    const nestedValidation = this.validateRecursive(value, depth + 1, visited);
    if (!nestedValidation.isValid) {
      warnings.add(
        `Invalid nested object in parameter ${key}: ${nestedValidation.warnings.join(', ')}`
      );
      return {
        sanitized: null,
        hasSecrets: nestedValidation.hasSecrets,
        isValid: false,
      };
    }
    return {
      sanitized: nestedValidation.sanitizedParams,
      hasSecrets: nestedValidation.hasSecrets,
      isValid: true,
    };
  }

  private static readonly MAX_CONTENT_SIZE = 10_000_000;

  /**
   * Scan a string for secrets and replace them with `[REDACTED-*]` tokens.
   *
   * @example
   * ```ts
   * ContentSanitizer.sanitizeContent('key: ghp_abc123xyz');
   * // → { content: 'key: [REDACTED-GITHUBTOKENS]', hasSecrets: true, ... }
   * ```
   */
  public static sanitizeContent(
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

    if (content.length > this.MAX_CONTENT_SIZE) {
      return {
        content: '[CONTENT-REDACTED-SIZE-LIMIT]',
        hasSecrets: true,
        secretsDetected: ['content-size-exceeded'],
        warnings: [
          `Content exceeds ${this.MAX_CONTENT_SIZE} character limit — redacted for safety`,
        ],
      };
    }

    const secretsResult = this.detectSecrets(
      content,
      filePath,
      getAllPatterns(patterns)
    );

    return {
      content: secretsResult.sanitizedContent,
      hasSecrets: secretsResult.hasSecrets,
      secretsDetected: secretsResult.secretsDetected,
      warnings:
        secretsResult.secretsDetected.length > 0
          ? [`${secretsResult.secretsDetected.length} secret(s) redacted`]
          : [],
    };
  }

  private static readonly CHUNK_SIZE = 500_000;
  private static readonly CHUNK_OVERLAP = 1_000;

  private static detectSecrets(
    content: string,
    filePath?: string,
    patterns: SensitiveDataPattern[] = allRegexPatterns
  ): {
    hasSecrets: boolean;
    secretsDetected: string[];
    sanitizedContent: string;
  } {
    if (content.length > this.CHUNK_SIZE) {
      return this.detectSecretsChunked(content, filePath, patterns);
    }
    return this.detectSecretsInChunk(content, filePath, patterns);
  }

  private static detectSecretsChunked(
    content: string,
    filePath: string | undefined,
    patterns: SensitiveDataPattern[]
  ): {
    hasSecrets: boolean;
    secretsDetected: string[];
    sanitizedContent: string;
  } {
    const secretsDetectedSet = new Set<string>();
    let sanitizedContent = content;

    for (const pattern of patterns) {
      if (
        pattern.fileContext &&
        (!filePath || !pattern.fileContext.test(filePath))
      ) {
        continue;
      }
      try {
        let chunkStart = 0;
        while (chunkStart < sanitizedContent.length) {
          const chunkEnd = Math.min(
            chunkStart + this.CHUNK_SIZE,
            sanitizedContent.length
          );
          const chunk = sanitizedContent.slice(chunkStart, chunkEnd);
          const chunkMatches = chunk.match(pattern.regex);
          if (chunkMatches && chunkMatches.length > 0) {
            secretsDetectedSet.add(pattern.name);
            const replacement = `[REDACTED-${pattern.name.toUpperCase()}]`;
            for (const m of chunkMatches) {
              const idx = sanitizedContent.indexOf(m, chunkStart);
              if (idx !== -1) {
                sanitizedContent =
                  sanitizedContent.slice(0, idx) +
                  replacement +
                  sanitizedContent.slice(idx + m.length);
              }
            }
          }
          const next = chunkEnd - this.CHUNK_OVERLAP;
          if (next <= chunkStart) break;
          chunkStart = next;
        }
      } catch {
        return {
          hasSecrets: true,
          secretsDetected: ['detection-error'],
          sanitizedContent: '[CONTENT-REDACTED-DETECTION-ERROR]',
        };
      }
    }

    const secretsDetected = Array.from(secretsDetectedSet);
    return {
      hasSecrets: secretsDetected.length > 0,
      secretsDetected,
      sanitizedContent,
    };
  }

  private static detectSecretsInChunk(
    content: string,
    filePath: string | undefined,
    patterns: SensitiveDataPattern[]
  ): {
    hasSecrets: boolean;
    secretsDetected: string[];
    sanitizedContent: string;
  } {
    let sanitizedContent = content;
    const secretsDetectedSet = new Set<string>();

    try {
      for (const pattern of patterns) {
        if (pattern.fileContext) {
          if (!filePath || !pattern.fileContext.test(filePath)) {
            continue;
          }
        }

        const matches = sanitizedContent.match(pattern.regex);
        if (matches && matches.length > 0) {
          matches.forEach(_match => secretsDetectedSet.add(pattern.name));
          sanitizedContent = sanitizedContent.replace(
            pattern.regex,
            `[REDACTED-${pattern.name.toUpperCase()}]`
          );
        }
      }
    } catch {
      return {
        hasSecrets: true,
        secretsDetected: ['detection-error'],
        sanitizedContent: '[CONTENT-REDACTED-DETECTION-ERROR]',
      };
    }

    const secretsDetected = Array.from(secretsDetectedSet);

    return {
      hasSecrets: secretsDetected.length > 0,
      secretsDetected,
      sanitizedContent,
    };
  }

  /**
   * Recursively sanitize an object — strip secrets, block prototype pollution, enforce limits.
   *
   * @example
   * ```ts
   * const result = ContentSanitizer.validateInputParameters({
   *   query: 'search term',
   *   token: 'sk-proj-abc123xyz',
   * });
   * result.sanitizedParams; // { query: 'search term', token: '[REDACTED-...]' }
   * ```
   */
  public static validateInputParameters(
    params: Record<string, unknown>
  ): ValidationResult {
    return this.validateRecursive(params, 0, new WeakSet<object>());
  }

  private static readonly DANGEROUS_KEYS = new Set([
    '__proto__',
    'constructor',
    'prototype',
  ]);

  /** Classify a key — returns an error message if invalid/dangerous, null if safe. */
  private static validateKey(key: string): string | null {
    if (typeof key !== 'string' || key.trim() === '') {
      return `Invalid parameter key: ${key}`;
    }
    if (this.DANGEROUS_KEYS.has(key)) {
      return `Dangerous parameter key blocked: ${key}`;
    }
    return null;
  }

  /**
   * Type-dispatch a single entry through the appropriate sanitizer.
   * Returns { sanitized, hasSecrets, hasValidationErrors, skip } where
   * skip=true means the entry should be excluded from the output.
   */
  private static sanitizeValue(
    key: string,
    value: unknown,
    depth: number,
    visited: WeakSet<object>,
    warnings: Set<string>
  ): {
    sanitized: unknown;
    hasSecrets: boolean;
    hasValidationErrors: boolean;
    skip: boolean;
  } {
    if (typeof value === 'string') {
      const r = this.sanitizeStringValue(key, value, warnings);
      return {
        sanitized: r.sanitized,
        hasSecrets: r.hasSecrets,
        hasValidationErrors: false,
        skip: false,
      };
    }
    if (Array.isArray(value)) {
      const r = this.sanitizeArrayValue(key, value, depth, visited, warnings);
      return {
        sanitized: r.sanitized,
        hasSecrets: r.hasSecrets,
        hasValidationErrors: r.hasValidationErrors,
        skip: false,
      };
    }
    if (value !== null && typeof value === 'object') {
      const r = this.sanitizeNestedObject(
        key,
        value as Record<string, unknown>,
        depth,
        visited,
        warnings
      );
      return {
        sanitized: r.sanitized,
        hasSecrets: r.hasSecrets,
        hasValidationErrors: !r.isValid,
        skip: !r.isValid,
      };
    }
    return {
      sanitized: value,
      hasSecrets: false,
      hasValidationErrors: false,
      skip: false,
    };
  }

  private static validateRecursive(
    params: Record<string, unknown>,
    depth: number,
    visited: WeakSet<object>
  ): ValidationResult {
    if (!params || typeof params !== 'object') {
      return {
        sanitizedParams: {},
        isValid: false,
        hasSecrets: false,
        warnings: ['Invalid parameters: must be an object'],
      };
    }

    if (depth > 20) {
      return {
        sanitizedParams: {},
        isValid: false,
        hasSecrets: false,
        warnings: ['Maximum nesting depth exceeded'],
      };
    }

    if (visited.has(params)) {
      return {
        sanitizedParams: {},
        isValid: false,
        hasSecrets: false,
        warnings: ['Circular reference detected'],
      };
    }
    visited.add(params);

    const sanitizedParams: Record<string, unknown> = {};
    const warnings = new Set<string>();
    let hasSecrets = false;
    let hasValidationErrors = false;

    for (const [key, value] of Object.entries(params)) {
      const keyError = this.validateKey(key);
      if (keyError) {
        hasValidationErrors = true;
        warnings.add(keyError);
        continue;
      }

      const result = this.sanitizeValue(key, value, depth, visited, warnings);
      if (result.hasSecrets) hasSecrets = true;
      if (result.hasValidationErrors) hasValidationErrors = true;
      if (!result.skip) sanitizedParams[key] = result.sanitized;
    }

    return {
      sanitizedParams,
      isValid: !hasValidationErrors,
      hasSecrets,
      warnings: Array.from(warnings),
    };
  }
}
