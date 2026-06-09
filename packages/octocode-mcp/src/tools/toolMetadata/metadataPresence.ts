import { completeMetadata } from '@octocodeai/octocode-core';

export function isToolInMetadata(toolName: string): boolean {
  return Object.prototype.hasOwnProperty.call(completeMetadata.tools, toolName);
}
