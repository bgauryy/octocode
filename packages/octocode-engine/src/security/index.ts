export type {
  SensitiveDataPattern,
  SanitizationResult,
  ValidationResult,
  PathValidationResult,
  ToolResult,
  ToolSecurityContext,
  ISanitizer,
} from './types.js';

export { ContentSanitizer } from './contentSanitizer.js';
export { maskSensitiveData } from './mask.js';
export { getSecurityBackendStatus } from './native.js';

export {
  PathValidator,
  pathValidator,
  resetPathValidator,
} from './pathValidator.js';

export { validateCommand } from './commandValidator.js';

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

export {
  shouldIgnore,
  shouldIgnorePath,
  shouldIgnoreFile,
} from './ignoredPathFilter.js';

export {
  DISCOVERY_IGNORED_FILE_EXTENSIONS,
  DISCOVERY_IGNORED_FILE_NAMES,
  DISCOVERY_IGNORED_FOLDER_NAMES,
  getDiscoveryExtension,
  shouldIgnoreDiscoveryDir,
  shouldIgnoreDiscoveryFile,
} from './discoveryFilter.js';
export type { DiscoveryExtensionOptions } from './discoveryFilter.js';

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
