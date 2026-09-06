import { z } from 'zod';
import { LocalSearchQuerySchema as SharedLocalSearchQuerySchema } from '../../toolContract/input/resources/tools/localSearch.js';
import { createRelaxedBulkQuerySchema } from '../../scheme/fields.js';

export type LocalTextResultView =
  | 'paginated'
  | 'discovery'
  | 'detailed'
  | 'content'
  | 'files'
  | 'filesWithout'
  | 'countLines'
  | 'countMatches'
  | 'matchOnly';

export function toLegacyTextQuery(
  query: Record<string, unknown>,
  resultView: LocalTextResultView
): Record<string, unknown> {
  const { pageSize, reverse, ...input } = query;
  const legacy = {
    ...input,
    ...(pageSize !== undefined ? { itemsPerPage: pageSize } : {}),
    ...(reverse !== undefined ? { sortReverse: reverse } : {}),
  };
  if (
    resultView === 'paginated' ||
    resultView === 'discovery' ||
    resultView === 'detailed'
  ) {
    return { ...legacy, mode: resultView };
  }
  return { ...legacy, mode: 'paginated', output: resultView };
}

const [
  textQuerySchema,
  structuralPatternQuerySchema,
  structuralRuleQuerySchema,
  filesQuerySchema,
  treeQuerySchema,
] = SharedLocalSearchQuerySchema.options;

const structuralResultViewField = z
  .enum(['content', 'files', 'countMatches'])
  .optional()
  .default('content')
  .describe('Structural result shape; content is the default.');

// The upstream structural branch also exposes text-only result views. Tighten
// the public schema at this boundary so every advertised value reaches the
// structural runner successfully.
export const LocalSearchQuerySchema = z.union([
  textQuerySchema,
  structuralPatternQuerySchema.extend({
    resultView: structuralResultViewField,
  }),
  structuralRuleQuerySchema.extend({
    resultView: structuralResultViewField,
  }),
  filesQuerySchema,
  treeQuerySchema,
]);
export type LocalSearchQuery = z.infer<typeof LocalSearchQuerySchema>;

export const LocalSearchBulkQuerySchema = createRelaxedBulkQuerySchema(
  LocalSearchQuerySchema,
  { maxQueries: 5 }
);
