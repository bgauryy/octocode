import { SYSTEM_PROMPT } from '@octocodeai/octocode-core';
import { baseSchemaDescriptions, toolNames } from './input/resources/global.js';
import { DIRECT_TOOL_DISCOVERY_DEFINITIONS } from '../tools/directToolCatalog/toolCatalogDefinitions.js';

/** Shared prompt plus the canonical tools-core-owned executable catalog. */
export const localCompleteMetadata = {
  systemPrompt: SYSTEM_PROMPT,
  toolNames,
  baseSchema: baseSchemaDescriptions,
  tools: Object.fromEntries(
    DIRECT_TOOL_DISCOVERY_DEFINITIONS.map(definition => [
      definition.name,
      { description: definition.description },
    ])
  ),
};
