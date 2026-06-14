// Types
export type {
  SensitiveDataPattern,
  SanitizationResult,
  ValidationResult,
  PathValidationResult,
  ToolResult,
  ISanitizer,
} from './types.js';

// Core — Rust-powered hot path
export { ContentSanitizer } from './contentSanitizer.js';
export { maskSensitiveData } from './mask.js';

// Path utilities — Node.js native
export {
  PathValidator,
  pathValidator,
  resetPathValidator,
} from './pathValidator.js';

// Command validation
export { validateCommand } from './commandValidator.js';

// Security middleware
export {
  withSecurityValidation,
  withBasicSecurityValidation,
  configureSecurity,
} from './withSecurityValidation.js';
export type { SecurityDepsConfig } from './withSecurityValidation.js';

// Parameter extraction utilities
export {
  extractResearchFields,
  extractRepoOwnerFromParams,
} from './paramExtractors.js';

// Path/file filtering
export {
  shouldIgnore,
  shouldIgnorePath,
  shouldIgnoreFile,
} from './ignoredPathFilter.js';

export { redactPath } from './pathUtils.js';

// Constants
export {
  ALLOWED_COMMANDS,
  DANGEROUS_PATTERNS,
  PATTERN_DANGEROUS_PATTERNS,
} from './securityConstants.js';

export { IGNORED_PATH_PATTERNS } from './pathPatterns.js';
export { IGNORED_FILE_PATTERNS } from './filePatterns.js';

// Pattern list (shim — patterns live in Rust, this is for API compatibility)
export { allRegexPatterns } from './regexes/index.js';

// Registry
export { SecurityRegistry, securityRegistry } from './registry.js';
export type { ISecurityRegistry } from './registry.js';
