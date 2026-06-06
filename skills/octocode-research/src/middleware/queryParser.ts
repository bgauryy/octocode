import { z } from 'zod';


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

export function parseAndValidate<T>(
  query: Record<string, unknown>,
  schema: z.ZodType<T>
): T[] {
  if (query.queries && typeof query.queries === 'string') {
    try {
      const parsed = JSON.parse(query.queries);
      if (Array.isArray(parsed)) {
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
    }
  }

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


function formatZodError(error: z.ZodError): string {
  return error.issues
    .map((issue) => {
      const path = issue.path.join('.');
      return path ? `${path}: ${issue.message}` : issue.message;
    })
    .join('; ');
}

