import { z } from 'zod';
import { completeMetadata } from '@octocodeai/octocode-core';
import { MAX_CHAR_LENGTH } from '../../config.js';
import {
  clampedInt,
  contextLinesField,
  createRelaxedBulkQuerySchema,
  lineNumberField,
  minifyFieldWithSymbols,
  type MinifyMode,
} from '../../scheme/localSchemaOverlay.js';
import { validateFileContentExtractionMode } from '../../scheme/fileContentModeValidation.js';
import { STATIC_TOOL_NAMES } from '../toolNames.js';

const QUERY_DESCRIPTIONS = {
  ...completeMetadata.baseSchema,
  ...completeMetadata.tools[STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT]?.schema,
} as Record<string, string>;

const FetchContentQueryShape = z.object({
  id: z.string().optional().describe(QUERY_DESCRIPTIONS.id!),
  mainResearchGoal: z
    .string()
    .optional()
    .describe(QUERY_DESCRIPTIONS.mainResearchGoal!),
  researchGoal: z
    .string()
    .optional()
    .describe(QUERY_DESCRIPTIONS.researchGoal!),
  reasoning: z.string().optional().describe(QUERY_DESCRIPTIONS.reasoning!),
  path: z.string().describe(QUERY_DESCRIPTIONS.path!),
  fullContent: z.boolean().optional().describe(QUERY_DESCRIPTIONS.fullContent!),
  matchString: z.string().optional().describe(QUERY_DESCRIPTIONS.matchString!),
  matchStringIsRegex: z
    .boolean()
    .optional()
    .describe(QUERY_DESCRIPTIONS.matchStringIsRegex!),
  matchStringCaseSensitive: z
    .boolean()
    .optional()
    .describe(QUERY_DESCRIPTIONS.matchStringCaseSensitive!),
  startLine: lineNumberField.describe(QUERY_DESCRIPTIONS.startLine!),
  endLine: lineNumberField.describe(QUERY_DESCRIPTIONS.endLine!),
  contextLines: contextLinesField
    .default(5)
    .describe(QUERY_DESCRIPTIONS.contextLines!),
  charOffset: clampedInt(0, 100_000_000)
    .optional()
    .describe(QUERY_DESCRIPTIONS.charOffset!),
  charLength: clampedInt(1, MAX_CHAR_LENGTH)
    .optional()
    .describe(QUERY_DESCRIPTIONS.charLength!),
  minify: minifyFieldWithSymbols.describe(QUERY_DESCRIPTIONS.minify!),
});

export const LocalFetchContentQuerySchema = FetchContentQueryShape.superRefine(
  validateFileContentExtractionMode
);

export type FetchContentQuery = z.infer<typeof FetchContentQueryShape> & {
  minify: MinifyMode;
};

export const LocalFetchContentBulkQuerySchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
  FetchContentQueryShape,
  { maxQueries: 5 }
);
