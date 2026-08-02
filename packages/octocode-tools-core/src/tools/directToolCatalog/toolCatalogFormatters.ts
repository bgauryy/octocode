/**
 * Engine-free direct-tool catalog: schema/output text formatters and
 * output-field metadata. Split out of `toolCatalogDefinitions.ts` to keep that
 * registry module under the max-lines budget — both are re-exported by the
 * `directToolCatalog.meta.ts` barrel. See that file's header comment for the
 * full P3 engine-free rationale.
 */
import { z } from 'zod';
import {
  findDirectToolDefinition,
  getDirectToolCategory,
  type DirectToolAutoFilledField,
  type DirectToolMetadata,
} from './toolCatalogDefinitions.js';

export interface DirectToolOutputField {
  name: string;
  type: string;
  optional?: boolean;
}

const DIRECT_TOOL_BASE_AUTO_FILLED_FIELDS: readonly DirectToolAutoFilledField[] =
  ['id', 'researchGoal', 'reasoning'];

const DIRECT_TOOL_OUTPUT_FIELDS: readonly DirectToolOutputField[] = [
  {
    name: 'content',
    type: 'Array<{ type: string; text: string }>',
  },
  {
    name: 'structuredContent',
    type: 'object',
    optional: true,
  },
  {
    name: 'isError',
    type: 'boolean',
    optional: true,
  },
];

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
  const category = getDirectToolCategory(toolName);
  const fields = [...DIRECT_TOOL_BASE_AUTO_FILLED_FIELDS];

  if (category === 'GitHub' || category === 'Package') {
    fields.splice(1, 0, 'mainResearchGoal');
  }

  return fields;
}

export function getDirectToolOutputFields(): DirectToolOutputField[] {
  return DIRECT_TOOL_OUTPUT_FIELDS.map(field => ({ ...field }));
}

export function formatDirectToolOutputSchemaText(): string {
  return JSON.stringify(
    Object.fromEntries(
      DIRECT_TOOL_OUTPUT_FIELDS.map(field => [
        field.name,
        field.optional ? `${field.type} (optional)` : field.type,
      ])
    ),
    null,
    2
  );
}

export function getDirectToolDescription(
  toolName: string,
  metadata?: DirectToolMetadata | null
): string {
  return metadata?.tools?.[toolName]?.description ?? toolName;
}
