export const maskSensitiveData = (text: string) => text;
export const nativeMaskSensitiveData = (text: string) => text;
export const nativeSanitizeContent = (content: string) => ({
  content,
  warnings: [],
  hasSecrets: false,
  secretsDetected: [],
});
export const nativePatternCount = () => 0;

const _sanitizeContentNoop = (content: string) => ({
  content,
  warnings: [],
  hasSecrets: false,
  secretsDetected: [],
});

export const ContentSanitizer = {
  sanitizeContent: _sanitizeContentNoop,
  validateInputParameters: (params: Record<string, unknown>) => ({
    sanitizedParams: params,
    isValid: true,
    hasSecrets: false,
    warnings: [],
  }),
};

export class PathValidator {
  isAllowed(_path: string) {
    return true;
  }
  validate(path: string) {
    return { valid: true, isValid: true, sanitizedPath: path };
  }
}

export const pathValidator = new PathValidator();
export const resetPathValidator = () => pathValidator;

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
