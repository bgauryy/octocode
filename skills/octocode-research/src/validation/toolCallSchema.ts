import { z } from 'zod';


export const MAX_QUERIES = 3;


const querySchema = z.record(z.string(), z.unknown()).refine(
  (obj) => Object.keys(obj).length > 0,
  { message: 'Query object cannot be empty' }
);


const toolCallBodySchema = z.object({
  queries: z
    .array(querySchema)
    .min(1, 'At least one query is required')
    .max(MAX_QUERIES, `Maximum ${MAX_QUERIES} queries per request`),
});


type ToolCallBody = z.infer<typeof toolCallBodySchema>;


interface ValidationResult<T> {
  success: boolean;
  data?: T;
  error?: {
    message: string;
    details: z.core.$ZodIssue[];
  };
}

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

export function getValidationHints(
  toolName: string,
  error: { message: string; details: z.core.$ZodIssue[] }
): string[] {
  const hints = [error.message];

  const hasQueriesError = error.details.some(
    (d) => d.path.includes('queries') || d.message.includes('queries')
  );

  if (hasQueriesError) {
    hints.push('Body must contain: { "queries": [{ ... }] }');
  }

  hints.push(`Use GET /tools/info/${toolName} for schema`);

  return hints;
}
