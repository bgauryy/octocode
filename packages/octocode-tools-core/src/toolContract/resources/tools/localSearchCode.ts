import { z } from 'zod';

import type { ToolSpec } from '../../types.js';
import {
  buildObject,
  contextLines,
  DEFAULT_MATCH_CONTENT_LENGTH,
  defineTool,
  intRange,
  MAX_MATCH_COUNT,
  MAX_MATCH_CONTENT_LENGTH,
  MAX_SEARCH_ITEMS_PER_PAGE,
  metaFields,
  optionalPageNumber,
  pageNumber,
  StringArray,
} from './_toolkit.js';

export const localSearchCode: ToolSpec = defineTool({
  name: 'localSearchCode',
  type: 'Local',
  shortDescription:
    'Search local files for text, regex, or AST patterns to find file and line.',
  instructions: `Find local file+line anchors — not path/name lookup (localFindFiles) or tree shape (localViewStructure). Modes: discovery=paths, paginated=snippets, detailed=context, structural=AST.
text/regex need searchText (one string, not array); structural needs pattern XOR rule and rejects search knobs (langType is valid — scopes to that language's extensions). $$$ list captures are budgeted: metavarRanges gives pruned/truncated line anchors, captureText:true for verbatim. matchWindow/unique need output:"matchOnly"; maxMatchesPerFile pairs matchPage. Follow with localGetFileContent, lspGetSemantics.`,
  schema: {
    searchText:
      'The search pattern. regex:"fixed" for a literal match, "perl" for advanced features (lookaheads, backreferences), else "smart". (Unlike ghSearchCode/ghSearchRepos, where `keywords` is an array of ANDed terms — this is a single string.)',
    mode: '"paginated" snippets; "discovery" paths only; "detailed" snippets plus context; "structural" AST/code-shape search with pattern or rule. Structural matches return line/capture anchors that can feed lspGetSemantics when symbol identity matters. (Unrelated to ghSearchPullRequests\'s `content.patches.mode` — different concepts sharing this name.)',
    pattern:
      'Structural only: code-shaped AST pattern with $X (one node) or $$$ARGS (node list). Modifiers are part of the node — `function $NAME` does not match `async function` or `export function`; include the modifiers or use a YAML `kind` rule for modifier-agnostic matches. Use this to find syntax shape, then use lspGetSemantics for semantic proof.',
    rule: 'Structural only: YAML ast-grep rule for not/inside/has/all/any. Use for partial or relational AST queries before escalating matched anchors to lspGetSemantics.',
    regex: '"smart" (default), "fixed" (literal), or "perl" (advanced).',
    caseMode: '"smart" (default), "sensitive", or "insensitive".',
    wholeWord: 'Match whole words only (text/regex modes).',
    invertMatch: 'Return non-matching lines (text/regex modes).',
    multiline: '"off" (default), "on", or "dotall" (. spans newlines).',
    include: 'Glob(s) of files to include.',
    exclude: 'Glob(s) of files to exclude.',
    excludeDir: 'Directory names to prune from the walk.',
    noIgnore: 'Also search .gitignored files.',
    hidden: 'Include dot-files.',
    matchContentLength:
      'Characters of matched-line content kept per hit; longer lines are truncated (numeric default/bounds live in the schema).',
    maxFiles:
      'Text/regex: per-page file ceiling that preserves the full ranked set for later pages. Structural: native scan cap that can be lossy; stats report possible truncation.',
    output:
      '"content" (default) matches with line text; "files"/"filesWithout" return matching/non-matching paths; "countLines"/"countMatches" return per-file counts; "matchOnly" returns just the matched substring (required for unique/matchWindow). ("files" is unlike localViewStructure\'s `filesOnly`, which filters a directory listing to file entries.)',
    unique:
      'Needs output:"matchOnly". "list" returns each matched value once per file; "count" adds its frequency.',
    sort: '"relevance" (default), "matchCount", "path", "modified", "accessed", or "created".',
    sortReverse: 'Reverse the sort order.',
    rankingProfile:
      'Language-aware relevance tuning for sort:"relevance"; "auto" (default) detects, else a language id or "generic".',
    langType:
      'Restrict to a language/file type, e.g. "ts" (ripgrep --type; in structural mode it maps to that language\'s include globs).',
    captureText:
      'Structural only: return full verbatim capture text for `$$$` list metavars (bodies, arg lists). Default false — list-capture text is omitted from `metavars`, and `metavarRanges` entries are comment-pruned and truncated to keep results lean; ranges always remain as line anchors.',
    maxMatchesPerFile: 'Pairs with matchPage.',
    matchWindow: 'Requires output:"matchOnly".',
    matchPage: 'Per-file match page.',
  },
});

export const RipgrepQuerySchema = buildObject(localSearchCode.schema, {
  ...metaFields,
  // Optional because mode:"structural" supplies `pattern`/`rule` instead of
  // `searchText`. The superRefine below requires it for every other mode.
  // A SINGLE text/regex string (not an array) — unlike ghSearchCode/ghSearchRepos
  // `keywords`, which are arrays of ANDed terms.
  searchText: z.string().optional(),
  path: z.string(),
  mode: z
    .enum(['paginated', 'discovery', 'detailed', 'structural'])
    .default('paginated'),
  pattern: z.string().optional(),
  rule: z.string().optional(),
  regex: z.enum(['smart', 'fixed', 'perl']).default('smart'),
  caseMode: z.enum(['smart', 'sensitive', 'insensitive']).default('smart'),
  wholeWord: z.boolean().optional(),
  invertMatch: z.boolean().optional(),
  include: StringArray,
  exclude: StringArray,
  excludeDir: StringArray,
  noIgnore: z.boolean().optional(),
  hidden: z.boolean().optional(),
  contextLines: contextLines(),
  matchContentLength: intRange(1, MAX_MATCH_CONTENT_LENGTH).default(
    DEFAULT_MATCH_CONTENT_LENGTH
  ),
  maxMatchesPerFile: intRange(1, MAX_MATCH_COUNT).optional(),
  maxFiles: intRange(1, MAX_MATCH_COUNT).optional(),
  multiline: z.enum(['off', 'on', 'dotall']).default('off'),
  sort: z
    .enum([
      'relevance',
      'matchCount',
      'path',
      'modified',
      'accessed',
      'created',
    ])
    .default('relevance'),
  sortReverse: z.boolean().optional(),
  rankingProfile: z
    .enum([
      'auto',
      'typescript',
      'javascript',
      'rust',
      'python',
      'go',
      'java',
      'scala',
      'markdown',
      'json',
      'yaml',
      'generic',
    ])
    .default('auto'),
  langType: z.string().optional(),
  captureText: z.boolean().optional(),
  output: z
    .enum([
      'content',
      'files',
      'filesWithout',
      'countLines',
      'countMatches',
      'matchOnly',
    ])
    .default('content'),
  unique: z.enum(['off', 'list', 'count']).default('off'),
  matchWindow: intRange(0, 200).optional(),
  matchPage: optionalPageNumber(),
  itemsPerPage: intRange(1, MAX_SEARCH_ITEMS_PER_PAGE).optional(),
  page: pageNumber(),
}).superRefine((query, ctx) => {
  const isStructural = query.mode === 'structural';
  if (isStructural) {
    if (!query.pattern && !query.rule) {
      ctx.addIssue({
        code: 'custom',
        message:
          'mode:"structural" requires `pattern` (a code-shaped query) or `rule` (a YAML relational rule).',
        path: ['pattern'],
      });
    }
    if (query.pattern && query.rule) {
      ctx.addIssue({
        code: 'custom',
        message: '`pattern` and `rule` are mutually exclusive.',
        path: ['rule'],
      });
    }
    // ripgrep-only knobs are meaningless on an AST query — reject non-default
    // values so a mis-built call fails loudly instead of silently ignoring them.
    if (query.wholeWord)
      ctx.addIssue({
        code: 'custom',
        message: '`wholeWord` is not valid with mode:"structural".',
        path: ['wholeWord'],
      });
    if (query.invertMatch)
      ctx.addIssue({
        code: 'custom',
        message: '`invertMatch` is not valid with mode:"structural".',
        path: ['invertMatch'],
      });
    for (const field of ['regex', 'caseMode', 'multiline'] as const) {
      const def = field === 'multiline' ? 'off' : 'smart';
      if (query[field] !== def)
        ctx.addIssue({
          code: 'custom',
          message: `\`${field}\` is not valid with mode:"structural".`,
          path: [field],
        });
    }
    if (query.output !== 'content')
      ctx.addIssue({
        code: 'custom',
        message: '`output` is not valid with mode:"structural".',
        path: ['output'],
      });
    if (query.unique !== 'off')
      ctx.addIssue({
        code: 'custom',
        message: '`unique` is not valid with mode:"structural".',
        path: ['unique'],
      });
  } else {
    if (query.pattern || query.rule) {
      ctx.addIssue({
        code: 'custom',
        message: '`pattern`/`rule` require mode:"structural".',
        path: [query.pattern ? 'pattern' : 'rule'],
      });
    }
    if (!query.searchText) {
      ctx.addIssue({
        code: 'custom',
        message: '`searchText` is required unless mode:"structural".',
        path: ['searchText'],
      });
    }
  }
  // `matchWindow`/`unique` only apply to the matched substring, so they require
  // output:"matchOnly" (the enum makes every other pairing structurally
  // impossible, so no mutual-exclusion checks are needed here).
  if (query.matchWindow !== undefined && query.output !== 'matchOnly') {
    ctx.addIssue({
      code: 'custom',
      message: '`matchWindow` requires output:"matchOnly".',
      path: ['matchWindow'],
    });
  }
  if (query.unique !== 'off' && query.output !== 'matchOnly') {
    ctx.addIssue({
      code: 'custom',
      message: '`unique` requires output:"matchOnly".',
      path: ['unique'],
    });
  }
});
