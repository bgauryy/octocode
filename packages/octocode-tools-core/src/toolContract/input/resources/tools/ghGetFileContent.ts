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

export const ghGetFileContent: ToolSpec = defineTool({
  name: 'ghGetFileContent',
  type: 'Github',
  shortDescription:
    'Read a file or a specific region from a GitHub repository.',
  instructions: `Read a known GitHub path after search or structure discovery; a complete search snippet needs no reread. For large files, get a symbols outline, then an exact range or match. Read small config files whole with minify:"none".
Choose fullContent, matchString, or startLine+endLine. Partial content cannot prove absence; continue with the returned charOffset. matchedLines are LSP anchors; match ranges and offsets are not. type:"directory" returns localPath when local access is enabled; otherwise use ghSearch operation:"tree".`,
  schema: {
    startLine: 'Requires endLine; exclusive with fullContent/matchString.',
    endLine: 'Requires startLine; must be >= startLine.',
    fullContent: 'Whole small file; exclusive with range/match.',
    matchString: 'Anchor returning padded matchRanges and exact matchedLines.',
    matchStringIsRegex: 'Makes matchString a regex.',
    matchStringCaseSensitive: 'Case-sensitive matchString.',
    type: '"file" reads; "directory" materializes localPath when clone/local access is enabled—otherwise use ghSearch operation:"tree".',
    forceRefresh: 'Bypass the 24h fetch cache and re-read from GitHub.',
    charOffset: 'Copy pagination.nextCharOffset.',
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
