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

export const ghGetFileContent: ToolSpec = defineTool({
  name: 'ghGetFileContent',
  type: 'Github',
  shortDescription:
    'Read a file or a specific region from a GitHub repository.',
  instructions: `Use after ghSearchCode/ghViewRepoStructure when you know the repo path — not for discovery. If a ghSearchCode snippet already answers the question, STOP — don't re-read the file.
Least-cost path: unknown/large file → minify:"symbols" outline, then a region (matchString or startLine+endLine). fullContent only for genuinely small whole-file reads, never to scan. A small structured/config file (package.json, tsconfig, lockfile) → read it whole with minify:"none"/fullContent, don't minify it — compaction can elide object boundaries and break exact key/value membership. For a literal value or quote, read the defining region with minify:"none" and copy the exact bytes.
Every response reports the file's full size (totalLines + sourceChars) and isPartial. A matchString/region read is a SLICE, not the whole file — it can cut a nested object mid-way; never conclude a key/field/value is absent or empty from a partial slice: continue via charOffset/next, or re-read the whole small structured file (package.json/tsconfig/lockfile) with minify:"none". Report only bytes you fetched — never invent a field, value, or quote.
Param relations: choose one extraction mode: fullContent OR matchString OR startLine+endLine. matchString pairs with contextLines and returns matchRanges (padded windows, not the exact hit) plus matchedLines (exact hit lines — see matchString field); charOffset continues isPartial pages. type:"directory" materializes localPath for local tools when clone is enabled.`,
  schema: {
    startLine: 'Requires endLine; exclusive with fullContent/matchString.',
    endLine: 'Requires startLine; must be >= startLine.',
    fullContent:
      'Whole file; SMALL files only; exclusive with range/match. For large/unknown files use minify:symbols then a region instead.',
    matchString:
      'Anchor returning slices plus matchRanges ({start,end} padded windows) and matchedLines (exact hit line numbers).',
    matchStringIsRegex: 'Makes matchString a regex.',
    matchStringCaseSensitive:
      'Case-sensitive matchString (default insensitive).',
    type: '"file" reads; "directory" materializes localPath.',
    forceRefresh: 'Bypass the 24h fetch cache and re-read from GitHub.',
    charOffset: 'Use pagination.nextCharOffset.',
    minify: '"symbols" outline, "standard" compact, "none" exact.',
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
