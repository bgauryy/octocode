import { completeMetadata } from '@octocodeai/octocode-core';

const STALE_MCP_OUTPUT_GUIDANCE =
  /MCP returns bounded triage text in content\[\]\.text and the full typed object in structuredContent; [^.]+restores full YAML text for clients without structuredContent\./;

export const localCompleteMetadata = {
  ...completeMetadata,
  systemPrompt: completeMetadata.systemPrompt.replace(
    STALE_MCP_OUTPUT_GUIDANCE,
    'MCP returns complete YAML text in content[].text and the full typed object in structuredContent.'
  ),
};
