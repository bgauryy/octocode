/**
 * Shared helpers used across the normalize/* submodules: the `fail()`
 * diagnostic-throw shorthand and zod-error formatting.
 */
import { OqlValidationError, diagnostic } from '../diagnostics.js';

export function fail(...diagnostics: ReturnType<typeof diagnostic>[]): never {
  throw new OqlValidationError(diagnostics);
}

export function formatZodError(error: unknown): string {
  const e = error as { issues?: Array<{ path: unknown[]; message: string }> };
  if (e && Array.isArray(e.issues)) {
    return e.issues
      .map(i => `${i.path.join('.') || '(root)'}: ${i.message}`)
      .join('; ');
  }
  return 'Invalid OQL query.';
}
