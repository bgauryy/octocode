import { completeMetadata } from '@octocodeai/octocode-core';

export const DESCRIPTIONS: Record<string, string> = Object.fromEntries(
  Object.values(completeMetadata.tools).map(tool => [
    tool.name,
    tool.description,
  ])
);
