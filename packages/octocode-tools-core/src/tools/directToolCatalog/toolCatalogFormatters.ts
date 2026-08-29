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
import {
  LOCAL_ANALYZE_GRAPH_DESCRIPTION,
  LOCAL_ANALYZE_GRAPH_TOOL_NAME,
} from '../../toolContract/resources/tools/localAnalyzeGraph.js';

const DIRECT_TOOL_BASE_AUTO_FILLED_FIELDS: readonly DirectToolAutoFilledField[] =
  ['id', 'researchGoal', 'reasoning'];

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

export function getDirectToolDescription(
  toolName: string,
  metadata?: DirectToolMetadata | null
): string {
  if (toolName === LOCAL_ANALYZE_GRAPH_TOOL_NAME) {
    return LOCAL_ANALYZE_GRAPH_DESCRIPTION;
  }
  return metadata?.tools?.[toolName]?.description ?? toolName;
}
