import {
  getToolSchemaRelations,
  getToolSchemaVariants,
} from '@octocodeai/octocode-core/schemas';

export function getDirectToolSchemaRelations(toolName: string): string[] {
  return getToolSchemaRelations(toolName);
}

export function getDirectToolSchemaVariants(toolName: string) {
  return getToolSchemaVariants(toolName);
}
