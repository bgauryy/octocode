/**
 * Input preparation/normalization for the engine-free direct-tool catalog:
 * JSON parsing, envelope handling, default-field filling, field-name alias
 * folding, and unknown-field detection (with did-you-mean suggestions). Split
 * out of `directToolCatalog.meta.ts` (still the public barrel) — see that
 * file's header comment for the full P3 rationale.
 */
import { z } from 'zod';
import {
  DIRECT_TOOL_AUTO_FILLED_FIELDS,
  DirectToolInputError,
  findDirectToolDefinition,
  getDirectToolCategory,
  type DirectToolInput,
  type PrepareDirectToolInputOptions,
} from './toolCatalogDefinitions.js';
import {
  getDirectToolDisplayFields,
  isRecord,
} from './toolSchemaIntrospection.js';

export function prepareDirectToolInputFromJsonText(
  toolName: string,
  inputText: string | undefined,
  options: PrepareDirectToolInputOptions = {}
): DirectToolInput | null {
  if (typeof inputText !== 'string') {
    return null;
  }

  let rawPayload: unknown;
  try {
    rawPayload = JSON.parse(inputText) as unknown;
  } catch {
    throw new DirectToolInputError('Tool input must be valid JSON.');
  }

  return prepareDirectToolInput(toolName, rawPayload, options);
}

export function prepareDirectToolInput(
  toolName: string,
  rawPayload: unknown,
  options: PrepareDirectToolInputOptions = {}
): DirectToolInput {
  const payload = buildDirectToolPayload(toolName, rawPayload, options);
  const tool = findDirectToolDefinition(toolName);
  if (!tool) {
    throw new DirectToolInputError(`Unknown tool: ${toolName}`);
  }

  const result = tool.inputSchema.safeParse(payload);
  if (!result.success) {
    throw new DirectToolInputError(
      'Tool input does not match the expected schema.',
      formatDirectToolValidationIssues(result.error)
    );
  }

  return result.data as DirectToolInput;
}

export function formatDirectToolValidationIssues(error: z.ZodError): string[] {
  return error.issues.map(issue => {
    const path = issue.path.length > 0 ? issue.path.join('.') : 'input';
    return `${path}: ${issue.message}`;
  });
}

function buildDirectToolPayload(
  toolName: string,
  rawPayload: unknown,
  options: PrepareDirectToolInputOptions
): DirectToolInput {
  let queriesInput: unknown[] = [];

  if (Array.isArray(rawPayload)) {
    queriesInput = rawPayload;
  } else if (isRecord(rawPayload) && Array.isArray(rawPayload.queries)) {
    queriesInput = rawPayload.queries;
  } else if (isRecord(rawPayload)) {
    queriesInput = [rawPayload];
  } else {
    throw new DirectToolInputError(
      'Tool input must be a JSON object, an array of query objects, or { "queries": [...] }.'
    );
  }

  if (queriesInput.length === 0) {
    throw new DirectToolInputError('At least one query is required.');
  }

  const envelopeFields =
    isRecord(rawPayload) && Array.isArray(rawPayload.queries)
      ? Object.fromEntries(
          Object.entries(rawPayload).filter(([key]) => key !== 'queries')
        )
      : {};

  const processedQueries = queriesInput.map((query, index) =>
    applyDefaultQueryFields(
      toolName,
      index,
      normalizeQueryObject(toolName, query, index, options),
      { sourceLabel: options.sourceLabel }
    )
  );
  return { ...envelopeFields, queries: processedQueries };
}

function applyDefaultQueryFields(
  toolName: string,
  index: number,
  query: Record<string, unknown>,
  options: Pick<PrepareDirectToolInputOptions, 'sourceLabel'>
): Record<string, unknown> {
  const nextQuery = { ...query };
  const category = getDirectToolCategory(toolName);
  const sourceLabel = options.sourceLabel ?? 'direct tool execution';
  const defaultGoal = buildDefaultGoal(toolName, sourceLabel);

  if (typeof nextQuery.id !== 'string' || nextQuery.id.trim().length === 0) {
    nextQuery.id = `${toolName}-${index + 1}`;
  }

  if (category === 'GitHub' || category === 'Package') {
    if (
      typeof nextQuery.mainResearchGoal !== 'string' ||
      nextQuery.mainResearchGoal.trim().length === 0
    ) {
      nextQuery.mainResearchGoal = defaultGoal;
    }
  }

  if (
    typeof nextQuery.researchGoal !== 'string' ||
    nextQuery.researchGoal.trim().length === 0
  ) {
    nextQuery.researchGoal = defaultGoal;
  }

  if (
    typeof nextQuery.reasoning !== 'string' ||
    nextQuery.reasoning.trim().length === 0
  ) {
    nextQuery.reasoning = `Executed via ${sourceLabel} tool command`;
  }

  return nextQuery;
}

function buildDefaultGoal(toolName: string, sourceLabel: string): string {
  return `Execute ${toolName} via ${sourceLabel}`;
}

function editDistance(a: string, b: string): number {
  const rows = a.length + 1;
  const cols = b.length + 1;
  const dist: number[] = Array.from({ length: rows * cols }, () => 0);
  for (let i = 0; i < rows; i++) dist[i * cols] = i;
  for (let j = 0; j < cols; j++) dist[j] = j;
  for (let i = 1; i < rows; i++) {
    for (let j = 1; j < cols; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      dist[i * cols + j] = Math.min(
        dist[(i - 1) * cols + j]! + 1,
        dist[i * cols + j - 1]! + 1,
        dist[(i - 1) * cols + j - 1]! + cost
      );
    }
  }
  return dist[rows * cols - 1]!;
}

/**
 * Closest schema field for an unknown key, if plausibly a rename/typo.
 * Catches the measured first-contact misses (keywordsToSearch→keywords,
 * name→packageName, depth→maxDepth): substring containment counts as a
 * match, otherwise a scaled edit-distance threshold.
 */
function suggestField(
  unknown: string,
  schemaFields: ReadonlySet<string>
): string | undefined {
  const lower = unknown.toLowerCase();
  let bestContained: string | undefined;
  let bestContainedScore = Number.POSITIVE_INFINITY;
  let bestFuzzy: string | undefined;
  let bestFuzzyScore = Number.POSITIVE_INFINITY;
  for (const field of schemaFields) {
    const fieldLower = field.toLowerCase();
    const distance = editDistance(lower, fieldLower);
    // Containment (name⊂packageName, keywords⊂keywordsToSearch) is a rename
    // signal, not a typo — it outranks any edit-distance match.
    if (fieldLower.includes(lower) || lower.includes(fieldLower)) {
      if (distance < bestContainedScore) {
        bestContained = field;
        bestContainedScore = distance;
      }
      continue;
    }
    // Fuzzy matching on very short unknowns produces false friends
    // ('op' → 'id'); require 3+ chars before trusting edit distance.
    if (lower.length < 3) continue;
    const threshold = Math.max(2, Math.floor(field.length / 3));
    if (distance <= threshold && distance < bestFuzzyScore) {
      bestFuzzy = field;
      bestFuzzyScore = distance;
    }
  }
  return bestContained ?? bestFuzzy;
}

/**
 * Cross-tool field-name harmonization: the same concept is named differently
 * per tool in the core schemas (keywords vs keywordsToSearch, path vs
 * filePath, maxDepth vs depth…). These well-known renames are accepted as
 * aliases and folded to the canonical field instead of erroring or being
 * silently stripped. Real typos still hit the did-you-mean path.
 */
const TOOL_FIELD_ALIASES: Record<string, Record<string, string>> = {
  // Map alias → canonical schema field (never the reverse).
  ghSearchCode: { keywordsToSearch: 'keywords' },
  ghSearchRepos: { keywordsToSearch: 'keywords' },
  npmSearch: { name: 'packageName' },
  ghViewRepoStructure: { depth: 'maxDepth' },
  lspGetSemantics: { op: 'type', line: 'lineHint', path: 'uri' },
  localGetFileContent: { filePath: 'path' },
  localSearchCode: { keywordsToSearch: 'keywords' },
};

function applyFieldAliases(
  toolName: string,
  query: Record<string, unknown>
): Record<string, unknown> {
  const aliases = TOOL_FIELD_ALIASES[toolName];
  if (!aliases) return query;
  let next: Record<string, unknown> | undefined;
  for (const [alias, canonical] of Object.entries(aliases)) {
    if (alias in query && !(canonical in query)) {
      next ??= { ...query };
      next[canonical] = next[alias];
      delete next[alias];
    }
  }
  return next ?? query;
}

function normalizeQueryObject(
  toolName: string,
  query: unknown,
  queryIndex: number,
  options: Pick<
    PrepareDirectToolInputOptions,
    'onUnknownFields' | 'rejectUnknownFields'
  > = {}
): Record<string, unknown> {
  if (!isRecord(query)) {
    throw new DirectToolInputError(
      'Tool input must be a JSON object or an array of objects.'
    );
  }
  const aliasedQuery = applyFieldAliases(toolName, query);

  const schemaFields = new Set([
    ...getDirectToolDisplayFields(toolName)
      .filter(field => !field.name.includes('.'))
      .map(field => field.name),
    ...DIRECT_TOOL_AUTO_FILLED_FIELDS,
  ]);
  const exactQuery: Record<string, unknown> = {};
  const unknownFields: string[] = [];
  for (const [key, value] of Object.entries(aliasedQuery)) {
    if (schemaFields.has(key)) {
      exactQuery[key] = value;
      continue;
    }
    unknownFields.push(key);
  }

  if (unknownFields.length > 0 && schemaFields.size > 0) {
    options.onUnknownFields?.(unknownFields, queryIndex);
    if (options.rejectUnknownFields === true) {
      const suggestions = unknownFields
        .map(field => {
          const suggested = suggestField(field, schemaFields);
          return suggested ? `'${field}' → did you mean '${suggested}'?` : '';
        })
        .filter(Boolean);
      throw new DirectToolInputError(
        `Unknown field(s): ${unknownFields.join(', ')}`,
        [
          ...suggestions,
          `Remove unknown field(s) from query ${queryIndex + 1}: ${unknownFields.join(', ')}`,
          `Run tools ${toolName} --scheme to see valid fields.`,
        ]
      );
    }
  }

  return exactQuery;
}
