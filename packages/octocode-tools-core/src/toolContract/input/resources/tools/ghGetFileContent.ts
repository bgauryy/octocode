import { z } from 'zod';
import { PUBLIC_TOOL_DESCRIPTIONS } from '../../../descriptions.js';

import type { ToolSpec } from '../../types/index.js';
import {
  buildObject,
  charLength,
  charOffset,
  defineTool,
  intRange,
  lineNumber,
  MAX_CONTEXT_LINES,
  metaFields,
  validateFileContentExtractionMode,
} from './_toolkit.js';

export const ghGetFileContent: ToolSpec = defineTool({
  name: 'ghGetFileContent',
  type: 'Github',
  shortDescription:
    'Read a file or a specific region from a GitHub repository.',
  instructions: PUBLIC_TOOL_DESCRIPTIONS.ghGetFileContent,
  schema: {
    branch:
      'Exact branch, tag, or commit ref; defaults to the repository default branch.',
    startLine: 'Requires endLine; exclusive with fullContent/matchString.',
    endLine: 'Requires startLine; must be >= startLine.',
    fullContent:
      'Whole file, default minify:none; only explicit character windows paginate. Exclusive with range/match.',
    matchString:
      'Anchor returning padded matchRanges and exact matchedLines; preserves matched text without minification.',
    matchStringIsRegex: 'Makes matchString a regex.',
    matchStringCaseSensitive: 'Case-sensitive matchString.',
    type: '"file" reads; "directory" materializes localPath when clone/local access is enabled—otherwise use ghSearch operation:"tree".',
    forceRefresh: 'Bypass the 24h fetch cache and re-read from GitHub.',
    charOffset:
      'Character continuation offset; copy the complete returned next query.',
    minify:
      '"symbols" paginated outline (no range/match selectors), "standard" compact source, "none" unminified. Security redaction applies. Default none for fullContent, standard otherwise; matches force none.',
  },
});

export const FileContentQuerySchema = buildObject(ghGetFileContent.schema, {
  ...metaFields,
  owner: z.string(),
  repo: z.string(),
  branch: z.string().optional(),
  path: z.string(),
  startLine: lineNumber(),
  endLine: lineNumber(),
  fullContent: z.boolean().optional(),
  matchString: z.string().optional(),
  matchStringIsRegex: z.boolean().optional(),
  matchStringCaseSensitive: z.boolean().optional(),
  forceRefresh: z.boolean().optional(),
  type: z.enum(['file', 'directory']).default('file'),
  contextLines: intRange(0, MAX_CONTEXT_LINES).default(5),
  charOffset: charOffset(),
  charLength: charLength(),
  minify: z.enum(['none', 'standard', 'symbols']).default('standard'),
}).superRefine(validateFileContentExtractionMode);
