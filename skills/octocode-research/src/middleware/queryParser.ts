import { z } from 'zod';

/**
 * Custom error class for validation failures.
 * Carries HTTP status code for proper error responses.
 */
class ValidationError extends Error {
  statusCode: number;
  code: string;
  details: z.core.$ZodIssue[];

  constructor(message: string, details: z.core.$ZodIssue[] = []) {
    super(message);
    this.name = 'ValidationError';
    this.statusCode = 400;
    this.code = 'VALIDATION_ERROR';
    this.details = details;
  }
}

/**
 * Parse and validate query parameters using a Zod schema.
 * Returns the validated data wrapped in an array (for tool compatibility).
 *
 * @param query - Express request query object
 * @param schema - Zod schema for validation
 * @returns Array containing the validated query object
 * @throws ValidationError if validation fails
 */
export function parseAndValidate<T>(
  query: Record<string, unknown>,
  schema: z.ZodType<T>
): T[] {
  // Check for JSON-encoded queries array (batch mode)
  if (query.queries && typeof query.queries === 'string') {
    try {
      const parsed = JSON.parse(query.queries);
      if (Array.isArray(parsed)) {
        // Validate each item in the array
        const validated = parsed.map((item, index) => {
          const result = schema.safeParse(item);
          if (!result.success) {
            throw new ValidationError(
              `Validation failed for query[${index}]: ${formatZodError(result.error)}`,
              result.error.issues
            );
          }
          return result.data;
        });
        return validated;
      }
    } catch (e) {
      if (e instanceof ValidationError) throw e;
      // JSON.parse failed - fall through to single query mode silently
      // (avoid logging untrusted input in production)
    }
  }

  // Parse flat query params (single query mode)
  const cleanedQuery: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(query)) {
    if (key !== 'queries') {
      cleanedQuery[key] = value;
    }
  }

  const result = schema.safeParse(cleanedQuery);
  if (!result.success) {
    throw new ValidationError(
      formatZodError(result.error),
      result.error.issues
    );
  }

  return [result.data];
}

/**
 * Format Zod error into a human-readable string.
 */
function formatZodError(error: z.ZodError): string {
  return error.issues
    .map((issue) => {
      const path = issue.path.join('.');
      return path ? `${path}: ${issue.message}` : issue.message;
    })
    .join('; ');
}

