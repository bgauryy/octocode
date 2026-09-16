/**
 * Single import point for engine sanitization in tools-core.
 *
 * All consumers import `sanitizeContent` and `maskSensitiveData` from here —
 * never directly from `@octocodeai/octocode-engine/*`. This keeps the coupling
 * surface to one file: if the engine API changes, only this file updates.
 */

import { ContentSanitizer } from '@octocodeai/octocode-engine/contentSanitizer';
import { maskSensitiveData } from '@octocodeai/octocode-engine/mask';
import type { SanitizationResult } from '@octocodeai/octocode-engine/security';

export type { SanitizationResult };

export { maskSensitiveData };

/**
 * Sanitize arbitrary text content, optionally scoped to a file path.
 * Returns the full {@link SanitizationResult} so callers can inspect
 * `.content`, `.warnings`, and `.redacted` without further engine imports.
 */
export function sanitizeContent(
  content: string,
  filePath?: string
): SanitizationResult {
  return ContentSanitizer.sanitizeContent(content, filePath);
}
