import { z } from 'zod';
import { FetchContentQuerySchema as UpstreamFetchContentQuerySchema } from '@octocodeai/octocode-core/schemas';
import {
  clampedInt,
  contextLinesField,
  createRelaxedBulkQuerySchema,
  lineNumberField,
  minifyFieldWithSymbols,
  type MinifyMode,
  optionalMetaFields,
  withCoreSchemaDescriptions,
} from '../../scheme/localSchemaOverlay.js';
import { validateFileContentExtractionMode } from '../../scheme/fileContentModeValidation.js';
import { STATIC_TOOL_NAMES } from '../toolNames.js';

const FetchContentQueryBaseSchema = withCoreSchemaDescriptions(
  STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
  UpstreamFetchContentQuerySchema.omit({
    matchStringContextLines: true,
  } as const).extend({
    ...optionalMetaFields,
    startLine: lineNumberField,
    endLine: lineNumberField,
    contextLines: contextLinesField.default(5),
    charOffset: clampedInt(0, 100_000_000).optional(),
    charLength: clampedInt(1, 50_000).optional(),
    minify: minifyFieldWithSymbols,
  })
);

export const LocalFetchContentQuerySchema =
  FetchContentQueryBaseSchema.superRefine(validateFileContentExtractionMode);

// `minify` is optional at the type level: the schema default ("none") is
// applied at the MCP input boundary, while direct impl callers may omit it.
export type FetchContentQuery = Omit<
  z.infer<typeof FetchContentQueryBaseSchema>,
  'minify'
> & { minify?: MinifyMode };

export const LocalFetchContentBulkQuerySchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
  FetchContentQueryBaseSchema,
  { maxQueries: 5 }
);
