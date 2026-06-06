export type {
  SensitiveDataPattern,
  SanitizationResult,
  ValidationResult,
  PathValidationResult,
  ToolResult,
  ISanitizer,
} from './types.js';

export {
  PathValidator,
  pathValidator,
  reinitializePathValidator,
} from './pathValidator.js';

export { validateCommand } from './commandValidator.js';

export { ContentSanitizer } from './contentSanitizer.js';

export {
  withSecurityValidation,
  withBasicSecurityValidation,
  configureSecurity,
} from './withSecurityValidation.js';
export type { SecurityDepsConfig } from './withSecurityValidation.js';

export {
  extractResearchFields,
  extractRepoOwnerFromParams,
} from './paramExtractors.js';

export { maskSensitiveData } from './mask.js';

export {
  shouldIgnore,
  shouldIgnorePath,
  shouldIgnoreFile,
} from './ignoredPathFilter.js';

export { redactPath } from './pathUtils.js';

export {
  ALLOWED_COMMANDS,
  DANGEROUS_PATTERNS,
  PATTERN_DANGEROUS_PATTERNS,
} from './securityConstants.js';

export { IGNORED_PATH_PATTERNS } from './pathPatterns.js';

export { IGNORED_FILE_PATTERNS } from './filePatterns.js';

export { allRegexPatterns } from './regexes/index.js';

export { SecurityRegistry, securityRegistry } from './registry.js';
export type { ISecurityRegistry } from './registry.js';
