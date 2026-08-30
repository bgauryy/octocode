/**
 * Engine-free direct-tool catalog: schema/output text formatters and
 * output-field metadata. Split out of `toolCatalogDefinitions.ts` to keep that
 * registry module under the max-lines budget — both are re-exported by the
 * `directToolCatalog.meta.ts` barrel. See that file's header comment for the
 * full P3 engine-free rationale.
 */
import { z } from 'zod';
import {
  findToolOutputSchema,
  getToolOutputFields,
} from '@octocodeai/octocode-core/schemas';
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

export function formatDirectToolOutputSchemaText(toolName: string): string {
  const schema = findToolOutputSchema(toolName);
  return schema
    ? JSON.stringify(z.toJSONSchema(schema, { io: 'output' }), null, 2)
    : '{}';
}

export function getDirectToolOutputFields(toolName: string): string[] {
  return getToolOutputFields(toolName);
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
  metadata?: DirectToolMetadata | null
): string {
  return metadata?.tools?.[toolName]?.description ?? toolName;
}
