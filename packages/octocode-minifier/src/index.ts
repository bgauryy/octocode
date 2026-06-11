// Types
export type {
  CommentPatternGroup,
  Strategy,
  FileTypeMinifyConfig,
  MinifyResult,
} from './types/index.js';
export { MINIFY_CONFIG, INDENTATION_SENSITIVE_NAMES } from './types/index.js';

// Core strategies (exported for consumers that need fine-grained control)
export {
  removeComments,
  minifyConservativeCore,
  minifyAggressiveCore,
  minifyJsonCore,
  minifyJsonReadable,
  minifyCodeCore,
  minifyGeneralCore,
  minifyMarkdownCore,
  minifyCSSCore,
  minifyHTMLCore,
  minifyJavaScriptCore,
  minifyWithTerser,
  minifyCSSAsync,
  minifyHTMLAsync,
} from './core/strategies.js';

// Main minification API
export { minifyContentSync, minifyContent } from './core/minifier.js';

// Apply helpers (content-view and full)
export {
  applyMinification,
  applyContentViewMinification,
} from './core/apply.js';

// Signature / skeleton extraction
export {
  extractSignatures,
  SIGNATURES_ONLY_HINT,
  SUPPORTED_SIGNATURE_EXTENSIONS,
} from './signatures/extractSignatures.js';
export type {
  SignatureStrategy,
  KeptLine,
} from './signatures/extractSignatures.js';

// YAML serialization
export { jsonToYamlString } from './yaml/jsonToYamlString.js';
export type { YamlConversionConfig } from './yaml/jsonToYamlString.js';

// File extension utility (re-exported for consumers)
export { getExtension } from './utils/fileExtension.js';
export type { GetExtensionOptions } from './utils/fileExtension.js';
