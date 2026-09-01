import { PUBLIC_TOOL_DESCRIPTIONS } from '../../toolContract/descriptions.js';

export function isToolInMetadata(toolName: string): boolean {
  return Object.prototype.hasOwnProperty.call(
    PUBLIC_TOOL_DESCRIPTIONS,
    toolName
  );
}
