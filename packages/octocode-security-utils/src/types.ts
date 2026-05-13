/**
 * Core security types for octocode-security-utils package.
 */

/** Pattern definition for detecting sensitive data */
export interface SensitiveDataPattern {
  name: string;
  description: string;
  regex: RegExp;
  fileContext?: RegExp;
  matchAccuracy?: 'high' | 'medium';
}

/** Result of content sanitization */
export interface SanitizationResult {
  content: string;
  hasSecrets: boolean;
  secretsDetected: string[];
  warnings: string[];
}

/** Result of parameter validation */
export interface ValidationResult {
  sanitizedParams: Record<string, unknown>;
  isValid: boolean;
  hasSecrets: boolean;
  warnings: string[];
}

/** Result of path validation */
export interface PathValidationResult {
  isValid: boolean;
  error?: string;
  sanitizedPath?: string;
}

/**
 * Generic tool result format.
 * Compatible with MCP's CallToolResult: the `content` array may contain
 * text items (`{ type: "text", text: string }`), image items, or any
 * other framework-defined content shape.
 *
 * Using `text?: string` (optional) so that non-text content types
 * (images, audio, embedded resources) are assignable without casting.
 */
export interface ToolResult {
  content: Array<{ type: string; text?: string }>;
  isError?: boolean;
}

/**
 * Abstract sanitizer interface (Dependency Inversion Principle).
 *
 * Allows withSecurityValidation to depend on this stable abstraction
 * rather than the concrete ContentSanitizer implementation, satisfying
 * the Stable Dependencies Principle.
 */
export interface ISanitizer {
  sanitizeContent(content: string, filePath?: string): SanitizationResult;
  validateInputParameters(params: Record<string, unknown>): ValidationResult;
}

/**
 * Abstract workspace root resolver interface.
 * Allows consumers to depend on the abstraction rather than the
 * concrete resolveWorkspaceRoot function.
 */
export interface IWorkspaceRootResolver {
  (explicit?: string): string;
}
