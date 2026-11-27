/** User-facing protocol errors: `ValidationError`. Interfaces report their message verbatim. */
export class ValidationError extends Error {
  override name = 'ValidationError';
}

/** The text an interface reports for a failure: expected errors (ValidationError, plain Error, system errors) say their message; a defect names its type. */
export function describe(error: unknown): string {
  if (error instanceof Error && (error instanceof ValidationError || error.name === 'Error' || 'code' in error)) return error.message;
  const { name, message } = error instanceof Error ? error : { name: typeof error, message: String(error) };
  return `Internal error (${name}): ${message}`;
}
