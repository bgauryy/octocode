import { z } from 'zod';
import { FetchContentQuerySchema as UpstreamFetchContentQuerySchema } from '@octocodeai/octocode-core/schemas';
import {
  clampedInt,
  contextLinesField,
  createRelaxedBulkQuerySchema,
  describeField,
  lineNumberField,
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
    path: describeField(
      UpstreamFetchContentQuerySchema.shape.path,
      "File path to read. This tool reads file content only — for listing directory contents (file names, sizes, counts), use localViewStructure instead. Relative paths resolve against the server's working directory; absolute paths must be within an allowed root (home directory or ALLOWED_PATHS)."
    ),
    fullContent: describeField(
      UpstreamFetchContentQuerySchema.shape.fullContent,
      'Read the whole file. Mutually exclusive with matchString and startLine/endLine.'
    ),
    matchString: describeField(
      UpstreamFetchContentQuerySchema.shape.matchString,
      'Anchor text or regex — returns matching slices with contextLines lines of context around each match. Matching is case-insensitive by default; set matchStringCaseSensitive=true for an exact-case match.'
    ),
    matchStringIsRegex: describeField(
      UpstreamFetchContentQuerySchema.shape.matchStringIsRegex,
      'Treat matchString as a regex pattern.'
    ),
    matchStringCaseSensitive: describeField(
      UpstreamFetchContentQuerySchema.shape.matchStringCaseSensitive,
      'Enable case-sensitive matching for matchString. Default is case-insensitive. Pass true to require an exact case match.'
    ),
    startLine: describeField(
      lineNumberField,
      '1-based first line to include. Use with endLine; mutually exclusive with fullContent and matchString.'
    ),
    endLine: describeField(
      lineNumberField,
      '1-based last line to include. Use with startLine; mutually exclusive with fullContent and matchString.'
    ),
    contextLines: contextLinesField.default(5),
    charOffset: clampedInt(0, 100_000_000)
      .optional()
      .describe(
        'Character offset for content pagination. When a full-file or matchString read is truncated, re-call with the charOffset from the response pagination cursor to read the next chunk.'
      ),
    charLength: clampedInt(1, 50_000)
      .optional()
      .describe(
        'Character page size for content pagination. Use with charOffset to read exact chunks without losing data.'
      ),
    signaturesOnly: z
      .boolean()
      .optional()
      .describe(
        'Extract only the structural skeleton of the file: imports, function/class/interface/type signatures — bodies are dropped. Saves 80–95% tokens. Use for structure exploration; follow up with startLine/endLine to read specific bodies.'
      ),
    minify: z
      .boolean()
      .optional()
      .describe(
        'Control minification of returned content. Default true — comments and redundant whitespace are stripped for token efficiency. Pass false to get the raw unprocessed content (useful for debugging or when exact formatting matters).'
      ),
  })
);

export const LocalFetchContentQuerySchema =
  FetchContentQueryBaseSchema.superRefine(validateFileContentExtractionMode);

export type FetchContentQuery = z.infer<typeof FetchContentQueryBaseSchema>;

export const LocalFetchContentBulkQuerySchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
  FetchContentQueryBaseSchema,
  { maxQueries: 5 }
);
