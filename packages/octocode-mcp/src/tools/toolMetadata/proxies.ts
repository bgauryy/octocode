/**
 * Backward-compatible barrel for lazy tool metadata access proxies.
 *
 * Prefer importing from focused modules under this directory:
 * - ./names.js
 * - ./descriptions.js
 * - ./hints.js
 * - ./baseSchema.js
 * - ./genericErrorHints.js
 */

export { TOOL_NAMES } from './names.js';
export { BASE_SCHEMA } from './baseSchema.js';
export { DESCRIPTIONS } from './descriptions.js';
export { TOOL_HINTS, getToolHintsSync } from './hints.js';
export { GENERIC_ERROR_HINTS } from './genericErrorHints.js';
export { getGenericErrorHintsSync } from './hints.js';
export { isToolInMetadata } from './metadataPresence.js';
