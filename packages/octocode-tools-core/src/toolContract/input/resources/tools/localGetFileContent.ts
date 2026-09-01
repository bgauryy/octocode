import { z } from 'zod';

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

export const localGetFileContent: ToolSpec = defineTool({
  name: 'localGetFileContent',
  type: 'Local',
  shortDescription: 'Read a local file or a specific region.',
  instructions: `Read a known local path after search, find, or structure discovery. For large files, get a symbols outline, then an exact range or match. Read small config files whole with minify:"none".
Choose fullContent, matchString, or startLine+endLine. Partial content cannot prove absence; continue with the returned charOffset. matchedLines are LSP anchors; match ranges and character offsets are not. Report fetched bytes only.`,
  schema: {
    fullContent: 'Whole file; small files only; exclusive with range/match.',
    matchString: 'Anchor returning padded matchRanges and exact matchedLines.',
    matchStringIsRegex: 'Makes matchString a regex.',
    matchStringCaseSensitive: 'Case-sensitive matchString.',
    startLine: 'Requires endLine; exclusive with fullContent/matchString.',
    endLine: 'Requires startLine; must be >= startLine.',
    charOffset: 'Copy pagination.nextCharOffset.',
    minify: '"symbols" outline, "standard" compact, "none" exact.',
  },
});

export const FetchContentQuerySchema = buildObject(localGetFileContent.schema, {
  ...metaFields,
  path: z.string(),
  fullContent: z.boolean().optional(),
  matchString: z.string().optional(),
  matchStringIsRegex: z.boolean().optional(),
  matchStringCaseSensitive: z.boolean().optional(),
  startLine: lineNumber(),
  endLine: lineNumber(),
  contextLines: intRange(0, MAX_CONTEXT_LINES).default(5),
  charOffset: charOffset(),
  charLength: charLength(),
  minify: z.enum(['none', 'standard', 'symbols']).default('standard'),
}).superRefine(validateFileContentExtractionMode);
