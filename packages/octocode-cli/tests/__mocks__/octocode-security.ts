/**
 * Test-only stub for octocode-security.
 * Replaces the native Rust binary with pure-JS no-ops so vitest can import
 * the CLI and octocode-mcp sources without a compiled .node binary.
 */

export const maskSensitiveData = (text: string) => text;
export const nativeMaskSensitiveData = (text: string) => text;
export const nativeSanitizeContent = (content: string) => ({
  content,
  warnings: [],
});
export const nativePatternCount = () => 0;

export class ContentSanitizer {
  sanitizeContent(content: string) {
    return { content, warnings: [] };
  }
}

export class PathValidator {
  isAllowed(_path: string) {
    return true;
  }
  validate(_path: string) {
    return { valid: true };
  }
}

export const pathValidator = new PathValidator();
export const reinitializePathValidator = () => pathValidator;

export const validateCommand = (_cmd: string) => ({ valid: true });
export const normalizeCommandName = (cmd: string) => cmd;

export const withSecurityValidation =
  (_deps: unknown) =>
  (handler: (...args: unknown[]) => unknown) =>
  (...args: unknown[]) =>
    handler(...args);

export const withBasicSecurityValidation =
  (handler: (...args: unknown[]) => unknown) =>
  (...args: unknown[]) =>
    handler(...args);

export const configureSecurity = () => {};

export const shouldIgnore = () => false;
export const shouldIgnoreFile = () => false;
export const shouldIgnorePath = () => false;

export const redactPath = (p: string) => p;

export const extractResearchFields = (q: unknown) => q;
export const extractRepoOwnerFromParams = (p: unknown) => p;

export const ALLOWED_COMMANDS: string[] = [];
export const DANGEROUS_PATTERNS: string[] = [];
export const PATTERN_DANGEROUS_PATTERNS: string[] = [];
export const IGNORED_FILE_PATTERNS: string[] = [];
export const IGNORED_PATH_PATTERNS: string[] = [];

export const allRegexPatterns: unknown[] = [];

class SecurityRegistryImpl {
  version = 0;
  extraSecretPatterns: unknown[] = [];
}
export const SecurityRegistry = SecurityRegistryImpl;
export const securityRegistry = new SecurityRegistryImpl();
