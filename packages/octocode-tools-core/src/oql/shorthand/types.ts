/**
 * Public shorthand input types for `octocode search`.
 *
 * `SearchShorthand` is the flat, CLI-flag-shaped bag of parts the shorthand
 * lowering pipeline (see ../shorthand.ts) consumes; `ShorthandResult` is what
 * it produces (either a lowered OQL input or a typed error).
 */
import type { OqlInputBatch, OqlInputQuery } from '../types.js';

/** Corpus already classified by the caller (local path vs GitHub ref). */
export type ShorthandCorpus =
  | { kind: 'local'; path: string }
  | { kind: 'github'; repo: string; path?: string; ref?: string }
  | { kind: 'npm' };

export interface SearchShorthand {
  target?: string;
  view?: string;
  contentView?: string;
  search?: string;
  /** literal text term (used when no pattern/rule/regex flag is set) */
  text?: string;
  /** regex pattern (rust dialect unless pcre2) */
  regex?: string;
  pcre2?: boolean;
  /** structural AST pattern (requires lang) */
  pattern?: string;
  /** structural rule, already parsed from JSON (requires lang) */
  rule?: unknown;
  lang?: string;
  corpus: ShorthandCorpus;
  materialize?: 'never' | 'auto' | 'required';
  branch?: string;
  forceRefresh?: boolean;
  limit?: number;
  page?: number;
  itemsPerPage?: number;
  include?: string[];
  exclude?: string[];
  excludeDir?: string[];
  extension?: string;
  filename?: string;
  pathPattern?: string;
  hidden?: boolean;
  noIgnore?: boolean;
  minDepth?: number;
  maxDepth?: number;
  empty?: boolean;
  modifiedWithin?: string;
  modifiedBefore?: string;
  accessedWithin?: string;
  sizeGreater?: string;
  sizeLess?: string;
  permissions?: string;
  executable?: boolean;
  readable?: boolean;
  writable?: boolean;
  details?: boolean;
  showModified?: boolean;
  caseInsensitive?: boolean;
  caseSensitive?: boolean;
  wholeWord?: boolean;
  fixedString?: boolean;
  multiline?: boolean;
  multilineDotall?: boolean;
  filesOnly?: boolean;
  directoriesOnly?: boolean;
  filesWithoutMatch?: boolean;
  entryType?: 'file' | 'directory';
  countLinesPerFile?: boolean;
  countMatchesPerFile?: boolean;
  onlyMatching?: boolean;
  unique?: boolean;
  countUnique?: boolean;
  contextLines?: number;
  invertMatch?: boolean;
  matchWindow?: number;
  matchContentLength?: number;
  maxMatchesPerFile?: number;
  matchPage?: number;
  maxFiles?: number;
  sort?: string;
  sortReverse?: boolean;
  rankingProfile?: string;
  debugRanking?: boolean;
  matchString?: string;
  matchRegex?: boolean;
  matchCaseSensitive?: boolean;
  startLine?: number;
  endLine?: number;
  charOffset?: number;
  charLength?: number;
  fullContent?: boolean;
  tree?: boolean;
  includeSizes?: boolean;
  op?: string;
  symbol?: string;
  symbolKind?: string;
  uri?: string;
  line?: number;
  order?: number;
  orderDirection?: string;
  depth?: number;
  workspaceRoot?: string;
  format?: string;
  owner?: string;
  topic?: string[];
  stars?: string;
  forks?: string;
  goodFirstIssues?: string;
  license?: string;
  created?: string;
  updated?: string;
  closed?: string;
  mergedAt?: string;
  size?: string;
  match?: string[];
  archived?: boolean;
  visibility?: string;
  concise?: boolean;
  state?: string;
  author?: string;
  label?: string;
  prNumber?: number;
  base?: string;
  head?: string;
  draft?: boolean;
  commentsContent?: boolean;
  commitsContent?: boolean;
  deep?: boolean;
  patchFile?: string;
  reviewMode?: string;
  filePage?: number;
  commentPage?: number;
  commitPage?: number;
  since?: string;
  until?: string;
  patches?: boolean;
  baseRef?: string;
  headRef?: string;
  diffPath?: string;
  intent?: string;
  facets?: string[];
  proof?: string;
  proofLimit?: number;
  includePackets?: boolean;
  includeFacts?: boolean;
  includeEdges?: boolean;
}

export type ShorthandResult =
  { input: OqlInputQuery | OqlInputBatch } | { error: string };
