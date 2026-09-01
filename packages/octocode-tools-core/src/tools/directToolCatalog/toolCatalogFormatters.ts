/**
 * Engine-free direct-tool catalog: schema and metadata formatters. Split out
 * of `toolCatalogDefinitions.ts` to keep that
 * registry module under the max-lines budget — both are re-exported by the
 * `directToolCatalog.meta.ts` barrel. See that file's header comment for the
 * full P3 engine-free rationale.
 */
import { z } from 'zod';
import {
  findDirectToolDefinition,
  type DirectToolAutoFilledField,
  type DirectToolMetadata,
} from './toolCatalogDefinitions.js';

const DIRECT_TOOL_BASE_AUTO_FILLED_FIELDS: readonly DirectToolAutoFilledField[] =
  ['goal', 'reasoning'];

export function formatDirectToolSchemaText(toolName: string): string {
  const tool = findDirectToolDefinition(toolName);
  if (!tool) {
    return '{}';
  }

  try {
    return JSON.stringify(
      z.toJSONSchema(tool.inputSchema, { io: 'input' }),
      null,
      2
    );
  } catch {
    return JSON.stringify(
      z.toJSONSchema(tool.schema, { io: 'input' }),
      null,
      2
    );
  }
}

export function formatDirectToolMetadataSchemaText(
  schema: Record<string, string> | undefined
): string {
  return JSON.stringify(schema ?? {}, null, 2);
}

export function getDirectToolAutoFilledFields(toolName: string): string[] {
  if (!findDirectToolDefinition(toolName)) return [];
  const fields = [...DIRECT_TOOL_BASE_AUTO_FILLED_FIELDS];
  return fields;
}

export function getDirectToolDescription(
  toolName: string,
  _metadata?: DirectToolMetadata | null
): string {
  return findDirectToolDefinition(toolName)?.description ?? toolName;
}
