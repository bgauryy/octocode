/**
 * Example command-pattern construction for the engine-free direct-tool
 * catalog (`--scheme`/help output). Split out of `directToolCatalog.meta.ts`
 * (still the public barrel) — see that file's header comment for the full P3
 * rationale. Hand-authored per-tool queries live in
 * `toolCommandPatternQueries.ts`; this file falls back to a schema-derived
 * example when a tool has none.
 */
import { LSP_GET_SEMANTICS_TOOL_NAME } from '../lsp/shared/semanticTypes.js';
import {
  findDirectToolDefinition,
  type DirectToolCommandPattern,
  type DirectToolDisplayField,
} from './toolCatalogDefinitions.js';
import { buildKnownDirectToolCommandPatternQueries } from './toolCommandPatternQueries.js';
import {
  buildExampleValue,
  getDirectToolDisplayFields,
} from './toolSchemaIntrospection.js';

export function formatDirectToolCommandPattern(
  toolName: string,
  query: Record<string, unknown>
): string {
  return `tools ${toolName} --queries '${JSON.stringify(query)}'`;
}

export function buildDirectToolCommandPatterns(
  toolName: string
): DirectToolCommandPattern[] {
  const knownPatterns = buildKnownDirectToolCommandPatternQueries(toolName);
  if (knownPatterns.length > 0) {
    return knownPatterns.map(pattern => ({
      ...pattern,
      command: formatDirectToolCommandPattern(toolName, pattern.query),
    }));
  }

  const query = buildSchemaDerivedExampleQuery(toolName);
  if (Object.keys(query).length === 0) {
    return [];
  }

  return [
    {
      label: 'schema-derived',
      query,
      command: formatDirectToolCommandPattern(toolName, query),
    },
  ];
}

export function buildDirectToolExampleQuery(
  toolName: string
): Record<string, unknown> {
  return buildDirectToolCommandPatterns(toolName)[0]?.query ?? {};
}

function buildSchemaDerivedExampleQuery(
  toolName: string
): Record<string, unknown> {
  if (!findDirectToolDefinition(toolName)) {
    return {};
  }

  const fields = getDirectToolDisplayFields(toolName);
  const topLevelFields = fields.filter(field => !field.name.includes('.'));
  const sourceFields = selectCommandPatternFields(topLevelFields);
  const example: Record<string, unknown> = {};

  for (const field of sourceFields) {
    example[field.name] = buildExampleValue(field.name, field.type);
  }

  if (
    toolName.startsWith('lsp') &&
    fields.some(field => field.name === 'uri')
  ) {
    example.uri ??= 'uri';
  }

  if (toolName === LSP_GET_SEMANTICS_TOOL_NAME) {
    example.type ??= 'definition';
    example.symbolName ??= 'symbolName';
    example.lineHint ??= 1;
  }

  return example;
}

const COMMAND_PATTERN_MAX_OPTIONAL_FIELDS = 4;

const COMMAND_PATTERN_FIELD_PRIORITY: ReadonlyMap<string, number> = new Map([
  ['keywords', 10],
  ['keywordsToSearch', 11],
  ['query', 12],
  ['text', 13],
  ['packageName', 14],
  ['name', 15],
  ['uri', 20],
  ['type', 21],
  ['owner', 30],
  ['repo', 31],
  ['extension', 32],
  ['filename', 33],
  ['language', 34],
  ['path', 40],
  ['target', 50],
  ['from', 51],
  ['scope', 52],
  ['pattern', 60],
  ['rule', 61],
  ['op', 80],
  ['operation', 81],
  ['minify', 90],
]);

const LOW_SIGNAL_COMMAND_PATTERN_FIELDS: ReadonlySet<string> = new Set([
  'page',
  'itemsPerPage',
  'limit',
  'matchPage',
  'maxFiles',
  'maxMatchesPerFile',
  'matchContentLength',
  'responseCharLength',
  'responseCharOffset',
]);

function selectCommandPatternFields(
  fields: readonly DirectToolDisplayField[]
): DirectToolDisplayField[] {
  const requiredFields = fields.filter(field => field.required);
  const selected = new Map<string, DirectToolDisplayField>();

  for (const field of requiredFields) {
    selected.set(field.name, field);
  }

  const optionalCandidates = fields
    .filter(field => !field.required && isUsefulCommandPatternField(field))
    .sort(compareCommandPatternFields);
  const optionalLimit = Math.max(
    COMMAND_PATTERN_MAX_OPTIONAL_FIELDS,
    requiredFields.length
  );

  for (const field of optionalCandidates) {
    if (selected.size >= optionalLimit) {
      break;
    }
    selected.set(field.name, field);
  }

  if (selected.size > 0) {
    return [...selected.values()];
  }

  return fields
    .filter(field => !LOW_SIGNAL_COMMAND_PATTERN_FIELDS.has(field.name))
    .filter(field => field.type !== 'boolean')
    .slice(0, COMMAND_PATTERN_MAX_OPTIONAL_FIELDS);
}

function isUsefulCommandPatternField(field: DirectToolDisplayField): boolean {
  if (LOW_SIGNAL_COMMAND_PATTERN_FIELDS.has(field.name)) {
    return false;
  }

  if (field.type === 'boolean') {
    return false;
  }

  if (hasDisplayFieldDefault(field)) {
    return COMMAND_PATTERN_FIELD_PRIORITY.has(field.name);
  }

  return true;
}

function compareCommandPatternFields(
  left: DirectToolDisplayField,
  right: DirectToolDisplayField
): number {
  const leftPriority =
    COMMAND_PATTERN_FIELD_PRIORITY.get(left.name) ?? Number.MAX_SAFE_INTEGER;
  const rightPriority =
    COMMAND_PATTERN_FIELD_PRIORITY.get(right.name) ?? Number.MAX_SAFE_INTEGER;

  if (leftPriority !== rightPriority) {
    return leftPriority - rightPriority;
  }

  return left.name.localeCompare(right.name);
}

function hasDisplayFieldDefault(field: DirectToolDisplayField): boolean {
  return field.constraints?.includes('default ') === true;
}
