import { toolSpecs } from '../../toolContract/resources/tools/index.js';

export function isToolInMetadata(toolName: string): boolean {
  return Object.prototype.hasOwnProperty.call(toolSpecs, toolName);
}
