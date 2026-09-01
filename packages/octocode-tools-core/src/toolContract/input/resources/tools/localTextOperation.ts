import { z } from 'zod';

import {
  buildObject,
  contextLines,
  DEFAULT_MATCH_CONTENT_LENGTH,
  intRange,
  MAX_MATCH_COUNT,
  MAX_MATCH_CONTENT_LENGTH,
  MAX_LOCAL_DEPTH,
  MAX_SEARCH_ITEMS_PER_PAGE,
  metaFields,
  optionalPageNumber,
  pageNumber,
  StringArray,
} from './_toolkit.js';

export const localTextOperationDescriptions = {
  searchText:
    'Single lexical pattern. regex:"fixed" is literal and "perl" enables advanced syntax.',
  mode: 'Search mode: "discovery" paths; "paginated" snippets; "detailed" context; "structural" AST.',
  pattern:
    'Structural code pattern: $X matches one node; $$$ARGS matches a list. Include modifiers or use a kind rule.',
  rule: 'Structural YAML ast-grep rule for relational matches.',
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
    'Matched-line characters kept per hit; longer lines are truncated.',
  maxFiles:
    'Text: page ceiling. Structural: scan cap; stats report truncation.',
  maxDepth: 'Maximum levels below the search root; 0 means root files.',
  output:
    '"content" lines; "files"/"filesWithout" paths; count modes totals; "matchOnly" substrings.',
  unique: 'With output:"matchOnly", deduplicate values or count frequency.',
  sort: '"relevance" (default), "matchCount", "path", "modified", "accessed", or "created".',
  sortReverse: 'Reverse the sort order.',
  rankingProfile: 'Language tuning for sort:"relevance"; "auto" detects.',
  langType:
    'Language/file type, e.g. "ts"; structural mode maps it to include globs.',
  captureText:
    'Structural: include verbatim $$$ list captures; ranges remain available when omitted.',
  maxMatchesPerFile: 'Pairs with matchPage.',
  matchWindow: 'Requires output:"matchOnly".',
  matchPage: 'Per-file match page.',
};

export const RipgrepQuerySchema = buildObject(localTextOperationDescriptions, {
  ...metaFields,
  // Optional because mode:"structural" supplies `pattern`/`rule` instead of
  // `searchText`. The superRefine below requires it for every other mode.
  // A single text/regex string, unlike GitHub keyword arrays.
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
  maxDepth: intRange(0, MAX_LOCAL_DEPTH).optional(),
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
        message: 'Set pattern or rule for structural mode.',
        path: ['pattern'],
      });
    }
    if (query.pattern && query.rule) {
      ctx.addIssue({
        code: 'custom',
        message: 'Choose pattern or rule, not both.',
        path: ['rule'],
      });
    }
    // ripgrep-only knobs are meaningless on an AST query — reject non-default
    // values so a mis-built call fails loudly instead of silently ignoring them.
    if (query.wholeWord)
      ctx.addIssue({
        code: 'custom',
        message: 'Remove wholeWord in structural mode.',
        path: ['wholeWord'],
      });
    if (query.invertMatch)
      ctx.addIssue({
        code: 'custom',
        message: 'Remove invertMatch in structural mode.',
        path: ['invertMatch'],
      });
    for (const field of ['regex', 'caseMode', 'multiline'] as const) {
      const def = field === 'multiline' ? 'off' : 'smart';
      if (query[field] !== def)
        ctx.addIssue({
          code: 'custom',
          message: `Remove ${field} in structural mode.`,
          path: [field],
        });
    }
    if (query.output !== 'content')
      ctx.addIssue({
        code: 'custom',
        message: 'Remove output in structural mode.',
        path: ['output'],
      });
    if (query.unique !== 'off')
      ctx.addIssue({
        code: 'custom',
        message: 'Remove unique in structural mode.',
        path: ['unique'],
      });
  } else {
    if (query.pattern || query.rule) {
      ctx.addIssue({
        code: 'custom',
        message: 'Use structural mode with pattern or rule.',
        path: [query.pattern ? 'pattern' : 'rule'],
      });
    }
    if (!query.searchText) {
      ctx.addIssue({
        code: 'custom',
        message: 'Set searchText for text modes.',
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
      message: 'Use output:"matchOnly" with matchWindow.',
      path: ['matchWindow'],
    });
  }
  if (query.unique !== 'off' && query.output !== 'matchOnly') {
    ctx.addIssue({
      code: 'custom',
      message: 'Use output:"matchOnly" with unique.',
      path: ['unique'],
    });
  }
});
