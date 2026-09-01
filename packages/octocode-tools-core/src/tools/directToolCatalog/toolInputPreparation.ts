/**
 * Input preparation/normalization for the engine-free direct-tool catalog:
 * JSON parsing, envelope handling, default-field filling, and unknown-field
 * detection (with did-you-mean suggestions). Split
 * out of `directToolCatalog.meta.ts` (still the public barrel) — see that
 * file's header comment for the full P3 rationale.
 */
import { z } from 'zod';
import {
  DIRECT_TOOL_AUTO_FILLED_FIELDS,
  DirectToolInputError,
  findDirectToolDefinition,
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
      'Check the query fields.',
      formatDirectToolValidationIssues(result.error)
    );
  }

  return result.data as DirectToolInput;
}

export function formatDirectToolValidationIssues(error: z.ZodError): string[] {
  const flattenIssue = (issue: z.core.$ZodIssue): z.core.$ZodIssue[] => {
    const unionErrors = (issue as z.core.$ZodIssue & { errors?: unknown })
      .errors;
    if (issue.code !== 'invalid_union' || !Array.isArray(unionErrors)) {
      return [issue];
    }
    const branches = unionErrors.filter(
      (branch): branch is z.core.$ZodIssue[] => Array.isArray(branch)
    );
    if (branches.length === 0) return [issue];
    return branches.reduce((best, branch) =>
      branch.length < best.length ? branch : best
    );
  };
  return error.issues.flatMap(flattenIssue).map(issue => {
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
      normalizeQueryObject(toolName, query, index, options),
      { sourceLabel: options.sourceLabel }
    )
  );
  return { ...envelopeFields, queries: processedQueries };
}

function applyDefaultQueryFields(
  toolName: string,
  query: Record<string, unknown>,
  options: Pick<PrepareDirectToolInputOptions, 'sourceLabel'>
): Record<string, unknown> {
  const nextQuery = { ...query };
  const sourceLabel = options.sourceLabel ?? 'direct tool execution';
  const defaultGoal = buildDefaultGoal(toolName, sourceLabel);

  if (
    typeof nextQuery.goal !== 'string' ||
    nextQuery.goal.trim().length === 0
  ) {
    nextQuery.goal = defaultGoal;
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

/** Rejection hints only: unknown names are never rewritten. Empty means no equivalent. */
const REJECTED_FIELD_HINTS: Readonly<
  Record<string, Readonly<Record<string, string>>>
> = {
  npmSearch: { name: 'packageName' },
  lspGetSemantics: {
    op: 'type',
    line: 'lineHint',
    path: 'uri',
    itemsPerPage: 'pageSize',
    limit: '',
  },
  localGetFileContent: { filePath: 'path' },
  localSearch: {
    itemsPerPage: 'pageSize',
    maxResults: 'limit',
    keywords: 'searchText',
    language: 'langType',
    regexType: 'regex',
    sortBy: 'sort',
    sortReverse: 'reverse',
  },
  ghGetFileContent: {
    matchStringContextLines: 'contextLines',
    minified: 'minify',
  },
  ghSearch: {
    itemsPerPage: 'pageSize',
    limit: '',
    topicsToSearch: 'topics',
  },
  ghListReleases: { itemsPerPage: 'pageSize', limit: '' },
  ghSearchDiscussions: {
    keywordsToSearch: 'keywords',
    itemsPerPage: 'pageSize',
    limit: '',
  },
  localAnalyzeGraph: {
    itemsPerPage: 'pageSize',
    maxResults: 'limit',
    maxDepth: 'depth',
  },
};

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
  const schemaFields = new Set([
    ...getDirectToolDisplayFields(toolName)
      .filter(field => !field.name.includes('.'))
      .map(field => field.name),
    ...collectDirectSchemaFieldNames(
      findDirectToolDefinition(toolName)?.schema
    ),
    ...DIRECT_TOOL_AUTO_FILLED_FIELDS,
  ]);
  const exactQuery: Record<string, unknown> = {};
  const unknownFields: string[] = [];
  for (const [key, value] of Object.entries(query)) {
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
          const suggested =
            REJECTED_FIELD_HINTS[toolName]?.[field] ??
            suggestField(field, schemaFields);
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

function collectDirectSchemaFieldNames(
  schema: z.ZodType | undefined
): string[] {
  if (!schema) return [];
  if (schema instanceof z.ZodObject) return Object.keys(schema.shape);
  const options = (schema as z.ZodType & { options?: unknown }).options;
  const unionOptions: z.ZodType[] =
    schema instanceof z.ZodUnion
      ? (schema.options as z.ZodType[])
      : Array.isArray(options)
        ? (options as z.ZodType[])
        : [];
  if (unionOptions.length > 0) {
    return [
      ...new Set<string>(
        unionOptions.flatMap((option: z.ZodType) =>
          collectDirectSchemaFieldNames(option as z.ZodType)
        )
      ),
    ];
  }
  return [];
}
