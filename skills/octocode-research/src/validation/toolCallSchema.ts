/**
 * Shared Zod schema for tool call body validation.
 *
 * Centralizes validation logic to avoid duplication across routes.
 *
 * @module validation/toolCallSchema
 */

import { z } from 'zod';

// =============================================================================
// Constants
// =============================================================================

/**
 * Maximum number of queries per tool call.
 * Limits parallel execution to prevent resource exhaustion.
 */
export const MAX_QUERIES = 3;

// =============================================================================
// Schemas
// =============================================================================

/**
 * Schema for individual query objects.
 * Each query must be a non-empty object with arbitrary key-value pairs.
 */
const querySchema = z.record(z.string(), z.unknown()).refine(
  (obj) => Object.keys(obj).length > 0,
  { message: 'Query object cannot be empty' }
);

/**
 * Schema for tool call request body.
 * Requires a queries array with 1-3 items.
 */
const toolCallBodySchema = z.object({
  queries: z
    .array(querySchema)
    .min(1, 'At least one query is required')
    .max(MAX_QUERIES, `Maximum ${MAX_QUERIES} queries per request`),
});

/**
 * Inferred type from the schema
 */
type ToolCallBody = z.infer<typeof toolCallBodySchema>;

// =============================================================================
// Validation Function
// =============================================================================

/**
 * Validation result type
 */
interface ValidationResult<T> {
  success: boolean;
  data?: T;
  error?: {
    message: string;
    details: z.core.$ZodIssue[];
  };
}

/**
 * Validate tool call body against schema.
 *
 * @param body - Request body to validate
 * @returns Validation result with data or error details
 *
 * @example
 * ```typescript
 * const result = validateToolCallBody(req.body);
 * if (!result.success) {
 *   res.status(400).json({
 *     success: false,
 *     hints: [result.error.message, ...result.error.details.map(d => d.message)]
 *   });
 *   return;
 * }
 * const { queries } = result.data;
 * ```
 */
export function validateToolCallBody(body: unknown): ValidationResult<ToolCallBody> {
  const result = toolCallBodySchema.safeParse(body);

  if (!result.success) {
    const issues = result.error.issues;
    const primaryMessage = issues[0]?.message || 'Invalid request body';

    return {
      success: false,
      error: {
        message: primaryMessage,
        details: issues,
      },
    };
  }

  return {
    success: true,
    data: result.data,
  };
}

/**
 * Generate user-friendly hints for validation errors.
 *
 * @param toolName - Name of the tool being called
 * @param error - Validation error details
 * @returns Array of hint strings for the user
 */
export function getValidationHints(
  toolName: string,
  error: { message: string; details: z.core.$ZodIssue[] }
): string[] {
  const hints = [error.message];

  // Add specific hints based on error type
  const hasQueriesError = error.details.some(
    (d) => d.path.includes('queries') || d.message.includes('queries')
  );

  if (hasQueriesError) {
    hints.push('Body must contain: { "queries": [{ ... }] }');
  }

  hints.push(`Use GET /tools/info/${toolName} for schema`);

  return hints;
}
