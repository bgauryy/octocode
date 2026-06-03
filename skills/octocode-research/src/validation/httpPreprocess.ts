/**
 * HTTP Query String Preprocessing Utilities
 *
 * HTTP query strings are always strings. These utilities convert them
 * to proper types before Zod schema validation.
 *
 * @module validation/httpPreprocess
 */

import { z } from 'zod';
import path from 'path';
import os from 'os';

// =============================================================================
// Preprocessors - Convert HTTP query strings to proper types
// =============================================================================

/**
 * Preprocess string to number (for query params)
 */
export const toNumber = (val: unknown): unknown => {
  if (typeof val === 'number') return val;
  if (typeof val === 'string' && /^\d+$/.test(val)) return parseInt(val, 10);
  return val;
};

/**
 * Preprocess string to boolean
 */
export const toBoolean = (val: unknown): unknown => {
  if (typeof val === 'boolean') return val;
  if (val === 'true') return true;
  if (val === 'false') return false;
  return val;
};

/**
 * Preprocess comma-separated string to array
 */
export const toArray = (val: unknown): unknown => {
  if (Array.isArray(val)) return val;
  if (typeof val === 'string') {
    if (val.trim() === '') return [];
    return val.split(',').map((s) => s.trim()).filter(Boolean);
  }
  return val;
};

// =============================================================================
// Reusable Schema Parts for HTTP
// =============================================================================

/** Numeric string that gets converted to number */
export const numericString = z.preprocess(toNumber, z.number().optional());

/** Required numeric string */
export const requiredNumber = z.preprocess(toNumber, z.number());

/** Boolean string that gets converted to boolean */
export const booleanString = z.preprocess(toBoolean, z.boolean().optional());

/** Comma-separated string that gets converted to array */
export const stringArray = z.preprocess(toArray, z.array(z.string()));

/**
 * URL-encoded traversal patterns to detect
 */
const URL_ENCODED_TRAVERSAL = [
  '%2e%2e',    // ..
  '%2e%2e%2f', // ../
  '%2e%2e%5c', // ..\
  '%252e',     // double-encoded .
  '%2f',       // /
  '%5c',       // \
] as const;

/**
 * Safe path that blocks traversal attacks.
 *
 * Validates:
 * - No directory traversal (..)
 * - No null bytes
 * - No Windows backslashes on non-Windows systems
 * - No URL-encoded traversal patterns
 */
export const safePath = z.string().refine(
  (p) => {
    // Check for null bytes
    if (p.includes('\0')) return false;

    // Normalize and check for traversal
    const normalized = path.normalize(p);
    if (normalized.includes('..')) return false;

    // Reject Windows backslashes on non-Windows (can bypass checks)
    if (os.platform() !== 'win32' && p.includes('\\')) return false;

    // Check for URL-encoded traversal patterns (case-insensitive)
    const lowerPath = p.toLowerCase();
    if (URL_ENCODED_TRAVERSAL.some((pattern) => lowerPath.includes(pattern))) {
      return false;
    }

    return true;
  },
  {
    message:
      'Path contains invalid characters or traversal patterns ' +
      '(null bytes, .., \\, URL-encoded sequences)',
  }
);

// =============================================================================
// Default Research Context
// =============================================================================

/**
 * Default research context values for HTTP requests
 */
const RESEARCH_DEFAULTS = {
  mainResearchGoal: 'HTTP API request',
  researchGoal: 'Execute tool via HTTP',
  reasoning: 'HTTP API call',
} as const;

let httpQueryCounter = 0;

/**
 * Apply research defaults and auto-generate `id`, ensuring structural compatibility
 * with MCP tool query types (which require `id`, `mainResearchGoal`, `researchGoal`, `reasoning`).
 */
export function withResearchDefaults<T extends Record<string, unknown>>(
  data: T
): Omit<T, 'id' | 'mainResearchGoal' | 'researchGoal' | 'reasoning'> & {
  id: string;
  mainResearchGoal: string;
  researchGoal: string;
  reasoning: string;
} {
  return {
    ...data,
    id: (data.id as string | undefined) ?? `http-${++httpQueryCounter}`,
    mainResearchGoal:
      (data.mainResearchGoal as string | undefined) ??
      RESEARCH_DEFAULTS.mainResearchGoal,
    researchGoal:
      (data.researchGoal as string | undefined) ??
      RESEARCH_DEFAULTS.researchGoal,
    reasoning:
      (data.reasoning as string | undefined) ?? RESEARCH_DEFAULTS.reasoning,
  };
}
