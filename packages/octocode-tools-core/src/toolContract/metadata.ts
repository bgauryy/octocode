import { completeMetadata as externalPromptMetadata } from '@octocodeai/octocode-core';

import { baseSchemaDescriptions, toolNames } from './resources/global.js';
import { toolSpecs } from './resources/tools/index.js';
import type { LocalCompleteMetadata } from './types.js';

/**
 * Repository-owned names, descriptions, and schemas. The external core package
 * supplies only the shared system prompt until that separate surface moves.
 */
export const localCompleteMetadata: LocalCompleteMetadata = {
  systemPrompt: externalPromptMetadata.systemPrompt,
  toolNames,
  baseSchema: baseSchemaDescriptions,
  tools: toolSpecs,
};
