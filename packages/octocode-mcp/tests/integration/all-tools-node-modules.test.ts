import { describe, it, expect } from 'vitest';
import { fetchContent } from '../../../octocode-tools-core/src/tools/local_fetch_content/fetchContent.js';
import { executeDirectTool } from '@octocodeai/octocode-tools-core';

type FetchContentResult = Awaited<ReturnType<typeof fetchContent>>;
import path from 'path';

const NODE_MODULES_PATH = path.resolve(process.cwd(), '../../node_modules');

type LocalSearchData = Record<string, unknown> & {
  status?: 'empty' | 'error';
  files?: unknown[];
  folders?: unknown[];
  entries?: unknown[];
  summary?: unknown;
  content?: string;
  structuredOutput?: string;
  pagination?: unknown;
  hints?: unknown[];
};

async function runLocalSearch(
  query: Record<string, unknown>
): Promise<LocalSearchData> {
  const response = await executeDirectTool('localSearch', { queries: [query] });
  expect(response.isError, JSON.stringify(response)).not.toBe(true);
  const result = (
    response.structuredContent as {
      results?: Array<{ data?: LocalSearchData }>;
    }
  )?.results?.[0]?.data;
  expect(result, JSON.stringify(response)).toBeDefined();
  return result ?? {};
}

type ToolResult = LocalSearchData | FetchContentResult;

function filePath(value: unknown): string | null {
  if (typeof value === 'string') return value;
  if (
    value &&
    typeof value === 'object' &&
    typeof (value as { path?: unknown }).path === 'string'
  ) {
    return (value as { path: string }).path;
  }
  return null;
}

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
  describe('localSearch operation:text - Pattern Search', () => {
    it('should find patterns in JavaScript files', async () => {
      const result = await runLocalSearch({
        operation: 'text',
        searchText: 'export',
        path: NODE_MODULES_PATH,
        include: ['*.js'],
        noIgnore: true,
        excludeDir: [],
        pageSize: 5,
      });

      verifySmartData(result, 'localSearch:text');

      if (result.status === undefined) {
        expect(result.files).toBeDefined();
        expect(Array.isArray(result.files)).toBe(true);
      }
    });

    it('should find files only mode', async () => {
      const result = await runLocalSearch({
        operation: 'text',
        searchText: 'package.json',
        path: NODE_MODULES_PATH,
        resultView: 'files',
        noIgnore: true,
        excludeDir: [],
        maxFiles: 10,
      });

      verifySmartData(result, 'localSearch:text');

      if (result.status === undefined) {
        expect(result.files).toBeDefined();
        expect(Array.isArray(result.files)).toBe(true);
      }
    });
  });

  describe('localSearch operation:tree - Directory Listing', () => {
    it('should list directory contents', async () => {
      const result = await runLocalSearch({
        operation: 'tree',
        path: NODE_MODULES_PATH,
        detail: 'basic',
        pageSize: 20,
      });

      verifySmartData(result, 'localSearch:tree');

      if (result.status === undefined) {
        expect(
          result.files ?? result.folders ?? result.entries ?? result.summary
        ).toBeDefined();
      }
    });

    it('should provide detailed file information', async () => {
      const result = await runLocalSearch({
        operation: 'tree',
        path: NODE_MODULES_PATH,
        detail: 'full',
        pageSize: 10,
        sort: 'size',
      });

      verifySmartData(result, 'localSearch:tree');

      if (result.status === undefined) {
        expect(
          result.files ?? result.folders ?? result.entries ?? result.summary
        ).toBeDefined();
      }
    });

    it('should generate tree view', async () => {
      const result = await runLocalSearch({
        operation: 'tree',
        path: NODE_MODULES_PATH,
        maxDepth: 2,
      });

      verifySmartData(result, 'localSearch:tree');

      if (result.status === undefined) {
        expect(
          result.files ?? result.folders ?? result.entries ?? result.summary
        ).toBeDefined();
      }
    });
  });

  describe('localSearch operation:files - File Discovery', () => {
    it('should find files by name', async () => {
      const result = await runLocalSearch({
        operation: 'files',
        path: NODE_MODULES_PATH,
        names: ['package.json'],
        maxDepth: 2,
        pageSize: 20,
      });

      verifySmartData(result, 'localSearch:files');

      if (result.status === undefined) {
        expect(result.files).toBeDefined();
        expect(Array.isArray(result.files)).toBe(true);
      }
    });

    it('should find files by extension', async () => {
      const result = await runLocalSearch({
        operation: 'files',
        path: NODE_MODULES_PATH,
        entryType: 'f',
        names: ['*.md'],
        pageSize: 10,
      });

      verifySmartData(result, 'localSearch:files');

      if (result.status === undefined) {
        expect(result.files).toBeDefined();
      }
    });

    it('should find directories', async () => {
      const result = await runLocalSearch({
        operation: 'files',
        path: NODE_MODULES_PATH,
        entryType: 'd',
        maxDepth: 1,
        pageSize: 15,
      });

      verifySmartData(result, 'localSearch:files');

      if (result.status === undefined) {
        expect(result.files).toBeDefined();
      }
    });
  });

  describe('localGetFileContent - File Content Reading', () => {
    let testFile: string | null = null;

    it('should find a test file first', async () => {
      const findResult = await runLocalSearch({
        operation: 'files',
        path: NODE_MODULES_PATH,
        names: ['package.json'],
        maxDepth: 2,
        pageSize: 5,
      });

      if (
        findResult.status === undefined &&
        findResult.files &&
        findResult.files.length > 0
      ) {
        const firstFile = findResult.files[0];
        testFile = filePath(firstFile);
      } else {
        const jsFileResult = await runLocalSearch({
          operation: 'files',
          path: NODE_MODULES_PATH,
          names: ['*.js'],
          pageSize: 1,
        });

        if (
          jsFileResult.status === undefined &&
          jsFileResult.files &&
          jsFileResult.files.length > 0
        ) {
          const firstJsFile = jsFileResult.files[0];
          testFile = filePath(firstJsFile);
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
        goal: 'Read full package.json content',
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
        goal: 'Read first 20 lines',
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
        goal: 'Extract dependencies section',
        reasoning: 'Testing pattern-based extraction',
      });

      verifySmartData(result, 'localGetFileContent');

      if (result.status === undefined) {
        expect(result.content).toBeDefined();
      }
    });
  });
});
