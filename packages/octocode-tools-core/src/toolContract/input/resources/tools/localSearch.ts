import { z } from 'zod';

import type { ToolSpec } from '../../types/index.js';
import { defineTool } from './_toolkit.js';
import {
  localTextOperationDescriptions,
  RipgrepQuerySchema,
} from './localTextOperation.js';
import {
  FindFilesQuerySchema,
  localFilesOperationDescriptions,
} from './localFilesOperation.js';
import {
  localTreeOperationDescriptions,
  ViewStructureQuerySchema,
} from './localTreeOperation.js';

const searchSchemaDescriptions: Record<string, string> = {
  ...localTextOperationDescriptions,
};
delete searchSchemaDescriptions.mode;
delete searchSchemaDescriptions.output;
delete searchSchemaDescriptions.itemsPerPage;
delete searchSchemaDescriptions.sortReverse;

const treeSchemaDescriptions: Record<string, string> = {
  ...localTreeOperationDescriptions,
};
delete treeSchemaDescriptions.recursive;

export const localSearch: ToolSpec = defineTool({
  name: 'localSearch',
  type: 'Local',
  shortDescription:
    'Search local text, syntax, paths, or directory trees through one operation.',
  instructions: `Choose operation:"text" for lexical anchors, "structural" for AST matches, "files" for path or metadata discovery, and "tree" for directory orientation. Each operation has a strict branch, so do not mix fields across operations. Files/tree prune common generated and vendor directories by default; pass excludeDir:[] to prune nothing. Read exact hits with localGetFileContent and prove symbol identity with lspGetSemantics.`,
  schema: {
    ...localFilesOperationDescriptions,
    ...treeSchemaDescriptions,
    ...searchSchemaDescriptions,
    operation: 'Required branch: "text", "structural", "files", or "tree".',
    resultView:
      'Text/structural result shape. Text also supports "paginated", "discovery", and "detailed".',
    pattern: 'Structural AST pattern.',
    namePattern: 'Tree entry name glob or substring.',
    pathRegex: 'Files operation: basename Rust regex.',
    limit: 'Files/tree discovery cap before pagination.',
    pageSize: 'Results returned per page.',
    sort: 'Operation-specific sort order.',
    reverse: 'Reverse the selected sort order.',
  },
});

const ripgrepObject = z.object(RipgrepQuerySchema.shape);
const findFilesObject = z.object(FindFilesQuerySchema.shape);

const textResultViewSchema = z.enum([
  'paginated',
  'discovery',
  'detailed',
  'content',
  'files',
  'filesWithout',
  'countLines',
  'countMatches',
  'matchOnly',
]);

const structuralResultViewSchema = z.enum([
  'content',
  'files',
  'countLines',
  'countMatches',
  'matchOnly',
]);

const textQuerySchema = ripgrepObject
  .omit({
    pattern: true,
    rule: true,
    captureText: true,
    mode: true,
    output: true,
    itemsPerPage: true,
    sortReverse: true,
  })
  .extend({
    operation: z.literal('text').describe('Use "text" for lexical search.'),
    searchText: z.string().describe('Single lexical pattern.'),
    resultView: textResultViewSchema
      .optional()
      .default('paginated')
      .describe('Text result shape; paginated is the default.'),
    pageSize: RipgrepQuerySchema.shape.itemsPerPage.describe(
      'Matches returned per page.'
    ),
    reverse: RipgrepQuerySchema.shape.sortReverse.describe(
      'Reverse the selected sort order.'
    ),
    unique: RipgrepQuerySchema.shape.unique.describe(
      'With resultView:"matchOnly", deduplicate values or count frequency.'
    ),
    matchWindow: RipgrepQuerySchema.shape.matchWindow.describe(
      'Requires resultView:"matchOnly".'
    ),
  })
  .strict();

const structuralBaseQuerySchema = ripgrepObject
  .omit({
    searchText: true,
    pattern: true,
    rule: true,
    mode: true,
    regex: true,
    caseMode: true,
    wholeWord: true,
    invertMatch: true,
    multiline: true,
    unique: true,
    matchWindow: true,
    output: true,
    itemsPerPage: true,
    sortReverse: true,
  })
  .extend({
    operation: z
      .literal('structural')
      .describe('Use "structural" for AST matching.'),
    resultView: structuralResultViewSchema
      .optional()
      .default('content')
      .describe('Structural result shape; content is the default.'),
    pageSize: RipgrepQuerySchema.shape.itemsPerPage.describe(
      'Matches returned per page.'
    ),
    reverse: RipgrepQuerySchema.shape.sortReverse.describe(
      'Reverse the selected sort order.'
    ),
  })
  .strict();

const structuralPatternQuerySchema = structuralBaseQuerySchema
  .extend({
    pattern: z.string().describe('Structural AST pattern.'),
  })
  .strict();

const structuralRuleQuerySchema = structuralBaseQuerySchema
  .extend({
    rule: z.string().describe('Structural YAML ast-grep rule.'),
  })
  .strict();

const filesQuerySchema = findFilesObject
  .omit({ regex: true, limit: true, sortBy: true, itemsPerPage: true })
  .extend({
    operation: z
      .literal('files')
      .describe('Use "files" for path and metadata discovery.'),
    pathRegex: FindFilesQuerySchema.shape.regex.describe(
      'Basename Rust regex.'
    ),
    limit: FindFilesQuerySchema.shape.limit.describe(
      'Post-sort discovery cap.'
    ),
    sort: FindFilesQuerySchema.shape.sortBy.describe('File sort order.'),
    pageSize: FindFilesQuerySchema.shape.itemsPerPage.describe(
      'Files returned per page.'
    ),
  })
  .strict();

const treeQuerySchema = ViewStructureQuerySchema.omit({
  pattern: true,
  recursive: true,
  limit: true,
  sortBy: true,
  itemsPerPage: true,
})
  .extend({
    operation: z
      .literal('tree')
      .describe('Use "tree" for directory orientation.'),
    namePattern: ViewStructureQuerySchema.shape.pattern.describe(
      'Entry name glob or substring.'
    ),
    limit: ViewStructureQuerySchema.shape.limit.describe(
      'Discovery cap before pagination.'
    ),
    sort: ViewStructureQuerySchema.shape.sortBy.describe('Tree sort order.'),
    pageSize: ViewStructureQuerySchema.shape.itemsPerPage.describe(
      'Entries returned per page.'
    ),
  })
  .strict();

function copyOperationIssues(
  result:
    { success: true; data: unknown } | { success: false; error: z.ZodError },
  ctx: z.RefinementCtx
): void {
  if ('error' in result) {
    for (const issue of result.error.issues) {
      // Branch schemas already validate public fields. Only carry forward
      // operation cross-field refinements.
      if (issue.code !== 'custom') continue;
      ctx.addIssue({
        code: 'custom',
        message: issue.message,
        path: issue.path,
      });
    }
  }
}

export const LocalSearchQuerySchema = z
  .union([
    textQuerySchema,
    structuralPatternQuerySchema,
    structuralRuleQuerySchema,
    filesQuerySchema,
    treeQuerySchema,
  ])
  .superRefine((query, ctx) => {
    if (query.operation === 'text') {
      const resultView = query.resultView;
      const executionInput: Record<string, unknown> = {
        ...query,
        itemsPerPage: query.pageSize,
        sortReverse: query.reverse,
      };
      delete executionInput.operation;
      delete executionInput.resultView;
      delete executionInput.pageSize;
      delete executionInput.reverse;
      const mapped = ['paginated', 'discovery', 'detailed'].includes(resultView)
        ? { ...executionInput, mode: resultView }
        : { ...executionInput, mode: 'paginated', output: resultView };
      copyOperationIssues(RipgrepQuerySchema.safeParse(mapped), ctx);
      return;
    }
    if (query.operation === 'structural') {
      return;
    }
    if (query.operation === 'files') {
      const executionInput: Record<string, unknown> = {
        ...query,
        regex: query.pathRegex,
        limit: query.limit,
        sortBy: query.sort,
        itemsPerPage: query.pageSize,
      };
      delete executionInput.operation;
      delete executionInput.pathRegex;
      delete executionInput.pageSize;
      delete executionInput.sort;
      copyOperationIssues(FindFilesQuerySchema.safeParse(executionInput), ctx);
    }
  });
