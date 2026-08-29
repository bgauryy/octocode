import { describe, it, expect } from 'vitest';
import { searchContentRipgrep } from '../../../octocode-tools-core/src/tools/local_ripgrep/searchContentRipgrep.js';
import { viewStructure } from '../../../octocode-tools-core/src/tools/local_view_structure/local_view_structure.js';
import { findFiles } from '../../../octocode-tools-core/src/tools/local_find_files/findFiles.js';
import { fetchContent } from '../../../octocode-tools-core/src/tools/local_fetch_content/fetchContent.js';
// Derive result types from the functions under test (the old public.js type
// aliases were removed in a refactor; deriving keeps this drift-proof).
type SearchContentResult = Awaited<ReturnType<typeof searchContentRipgrep>>;
type ViewStructureResult = Awaited<ReturnType<typeof viewStructure>>;
type FindFilesResult = Awaited<ReturnType<typeof findFiles>>;
type FetchContentResult = Awaited<ReturnType<typeof fetchContent>>;
import { RipgrepQuerySchema } from '@octocodeai/octocode-tools-core';
import path from 'path';

const NODE_MODULES_PATH = path.resolve(process.cwd(), 'node_modules');

const runRipgrep = (query: Record<string, unknown>) =>
  searchContentRipgrep(
    RipgrepQuerySchema.parse({
      id: 'test:ripgrep-integration',
      researchGoal: 'Test',
      reasoning: 'Integration test',
      ...query,
    })
  );

type ToolResult =
  | SearchContentResult
  | ViewStructureResult
  | FindFilesResult
  | FetchContentResult;

function verifySmartData<T extends ToolResult>(result: T, toolName: string): T {
  expect(result, `${toolName} should return a result object`).toBeDefined();
  expect(
    [undefined, 'empty', 'error'],
    `${toolName} status should be a valid envelope status`
  ).toContain(result.status);

  if (result.status === undefined) {
    const hasFiles =
      'files' in result &&
      Array.isArray(result.files) &&
      result.files.length > 0;
    const hasContent =
      'content' in result &&
      typeof result.content === 'string' &&
      result.content.length > 0;
    const hasStructuredOutput =
      'structuredOutput' in result &&
      typeof result.structuredOutput === 'string' &&
      result.structuredOutput.length > 0;
    const hasPagination = Boolean(result.pagination);
    const hasFolders =
      'folders' in result &&
      Array.isArray(result.folders) &&
      result.folders.length > 0;
    const hasSummary = 'summary' in result && Boolean(result.summary);

    expect(
      hasFiles ||
        hasContent ||
        hasStructuredOutput ||
        hasPagination ||
        hasFolders ||
        hasSummary,
      `${toolName} should have data when status indicates success`
    ).toBe(true);
  }

  if (result.hints) {
    expect(
      Array.isArray(result.hints),
      `${toolName} hints should be an array`
    ).toBe(true);
  }

  return result;
}

describe('Integration Tests: All Tools on node_modules', () => {
  describe('localSearchCode - Pattern Search', () => {
    it('should find patterns in JavaScript files', async () => {
      const result = await runRipgrep({
        searchText: 'export',
        path: NODE_MODULES_PATH,
        include: ['*.js'],
        itemsPerPage: 5,
        researchGoal: 'Find exported functions in JavaScript files',
        reasoning: 'Testing pattern search on node_modules',
      });

      verifySmartData(result, 'localSearchCode');

      if (result.status === undefined) {
        expect(result.files).toBeDefined();
        expect(Array.isArray(result.files)).toBe(true);
      }
    });

    it('should find files only mode', async () => {
      const result = await runRipgrep({
        searchText: 'package.json',
        path: NODE_MODULES_PATH,
        output: 'files',
        maxFiles: 10,
        researchGoal: 'Find package.json files',
        reasoning: 'Testing output:"files" mode',
      });

      verifySmartData(result, 'localSearchCode');

      if (result.status === undefined) {
        expect(result.files).toBeDefined();
        expect(Array.isArray(result.files)).toBe(true);
      }
    });
  });

  describe('localViewStructure - Directory Listing', () => {
    it('should list directory contents', async () => {
      const result = await viewStructure({
        path: NODE_MODULES_PATH,
        detail: 'basic',
        itemsPerPage: 20,
        researchGoal: 'List top-level node_modules contents',
        reasoning: 'Testing basic directory listing',
      });

      verifySmartData(result, 'localViewStructure');

      if (result.status === undefined) {
        expect(
          result.files ?? result.folders ?? result.entries ?? result.summary
        ).toBeDefined();
      }
    });

    it('should provide detailed file information', async () => {
      const result = await viewStructure({
        path: NODE_MODULES_PATH,
        detail: 'full',
        itemsPerPage: 10,
        sortBy: 'size',
        researchGoal: 'Get detailed file information sorted by size',
        reasoning: 'Testing detailed listing with sorting',
      });

      verifySmartData(result, 'localViewStructure');

      if (result.status === undefined) {
        expect(
          result.files ?? result.folders ?? result.entries ?? result.summary
        ).toBeDefined();
      }
    });

    it('should generate tree view', async () => {
      const result = await viewStructure({
        path: NODE_MODULES_PATH,
        maxDepth: 2,
        researchGoal: 'Get tree structure view',
        reasoning: 'Testing tree view mode',
      });

      verifySmartData(result, 'localViewStructure');

      if (result.status === undefined) {
        expect(
          result.files ?? result.folders ?? result.entries ?? result.summary
        ).toBeDefined();
      }
    });
  });

  describe('localFindFiles - File Discovery', () => {
    it('should find files by name', async () => {
      const result = await findFiles({
        path: NODE_MODULES_PATH,
        names: ['package.json'],
        maxDepth: 2,
        itemsPerPage: 20,
        researchGoal: 'Find package.json files',
        reasoning: 'Testing name-based file discovery',
      });

      verifySmartData(result, 'localFindFiles');

      if (result.status === undefined) {
        expect(result.files).toBeDefined();
        expect(Array.isArray(result.files)).toBe(true);
      }
    });

    it('should find files by extension', async () => {
      const result = await findFiles({
        path: NODE_MODULES_PATH,
        entryType: 'f',
        names: ['*.md'],
        itemsPerPage: 10,
        researchGoal: 'Find markdown documentation files',
        reasoning: 'Testing extension-based discovery',
      });

      verifySmartData(result, 'localFindFiles');

      if (result.status === undefined) {
        expect(result.files).toBeDefined();
      }
    });

    it('should find directories', async () => {
      const result = await findFiles({
        path: NODE_MODULES_PATH,
        entryType: 'd',
        maxDepth: 1,
        itemsPerPage: 15,
        researchGoal: 'Find top-level directories',
        reasoning: 'Testing directory discovery',
      });

      verifySmartData(result, 'localFindFiles');

      if (result.status === undefined) {
        expect(result.files).toBeDefined();
      }
    });
  });

  describe('localGetFileContent - File Content Reading', () => {
    let testFile: string | null = null;

    it('should find a test file first', async () => {
      const findResult = await findFiles({
        path: NODE_MODULES_PATH,
        names: ['package.json'],
        maxDepth: 2,
        itemsPerPage: 5,
        researchGoal: 'Find package.json files',
        reasoning: 'Testing file discovery for fetch_content tests',
      });

      if (
        findResult.status === undefined &&
        findResult.files &&
        findResult.files.length > 0
      ) {
        const firstFile = findResult.files[0];
        testFile =
          typeof firstFile === 'string' ? firstFile : (firstFile?.path ?? null);
      } else {
        const jsFileResult = await findFiles({
          path: NODE_MODULES_PATH,
          names: ['*.js'],
          itemsPerPage: 1,
        });

        if (
          jsFileResult.status === undefined &&
          jsFileResult.files &&
          jsFileResult.files.length > 0
        ) {
          const firstJsFile = jsFileResult.files[0];
          testFile =
            typeof firstJsFile === 'string'
              ? firstJsFile
              : (firstJsFile?.path ?? null);
        }
      }

      expect([undefined, 'empty', 'error']).toContain(findResult.status);
    });

    it('should read full file content', async () => {
      if (!testFile) {
        return;
      }

      const result = await fetchContent({
        path: testFile,
        fullContent: true,
        contextLines: 5,
        minify: 'standard',
        researchGoal: 'Read full package.json content',
        reasoning: 'Testing full content fetch',
      });

      verifySmartData(result, 'localGetFileContent');

      if (result.status === undefined) {
        expect(result.content).toBeDefined();
        expect(typeof result.content).toBe('string');
      }
    });

    it('should read line range', async () => {
      if (!testFile) {
        return;
      }

      const result = await fetchContent({
        path: testFile,
        charOffset: 0,
        charLength: 2000,
        contextLines: 5,
        minify: 'standard',
        researchGoal: 'Read first 20 lines',
        reasoning: 'Testing line range fetch',
      });

      verifySmartData(result, 'localGetFileContent');

      if (result.status === undefined) {
        expect(result.content).toBeDefined();
      }
    });

    it('should extract pattern-based content', async () => {
      if (!testFile) {
        return;
      }

      const result = await fetchContent({
        path: testFile,
        matchString: 'dependencies',
        contextLines: 5,
        minify: 'standard',
        researchGoal: 'Extract dependencies section',
        reasoning: 'Testing pattern-based extraction',
      });

      verifySmartData(result, 'localGetFileContent');

      if (result.status === undefined) {
        expect(result.content).toBeDefined();
      }
    });
  });
});
