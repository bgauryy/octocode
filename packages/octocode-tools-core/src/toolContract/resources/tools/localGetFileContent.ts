import { z } from 'zod';

import type { ToolSpec } from '../../types.js';
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
  instructions: `Use after localSearchCode/localViewStructure/localFindFiles when you have a path — not for discovery. Unknown/large file: minify:"symbols" first, then exact range or matchString. A small structured/config file (package.json, tsconfig, lockfile) → read it whole with minify:"none"/fullContent, don't minify it — compaction can elide object boundaries and break exact key/value membership. For a literal value or quote, read the defining region with minify:"none" and copy the exact bytes.
Every response reports the file's full size (totalLines + sourceChars) and isPartial. A matchString/region read is a SLICE, not the whole file — it can cut a nested object mid-way; never conclude a key/field/value is absent or empty from a partial slice: continue via charOffset/next, or re-read the whole small structured file with minify:"none". Report only bytes you fetched — never invent a field, value, or quote.
Param relations: choose one extraction mode: fullContent OR matchString OR startLine+endLine. matchString pairs with contextLines and returns matchRanges ({start,end} line windows padded by contextLines, NOT the exact hit) plus matchedLines (the exact matched line numbers); charOffset continues isPartial pages. Follow matchedLines/matchRanges into LSP.`,
  schema: {
    fullContent: 'Whole file; small files only; exclusive with range/match.',
    matchString:
      'Anchor returning slices plus matchRanges ({start,end} padded windows) and matchedLines (exact hit line numbers).',
    matchStringIsRegex: 'Makes matchString a regex.',
    matchStringCaseSensitive:
      'Case-sensitive matchString (default insensitive).',
    startLine: 'Requires endLine; exclusive with fullContent/matchString.',
    endLine: 'Requires startLine; must be >= startLine.',
    charOffset: 'Use pagination.nextCharOffset.',
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
