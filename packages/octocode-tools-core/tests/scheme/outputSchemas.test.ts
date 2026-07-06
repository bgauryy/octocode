/**
 * Output schema validation tests for local tools (Phase 2 of the
 * Tool Schema / Output / Pagination Alignment plan).
 *
 * These tests import the output schemas from each local tool's scheme.ts and
 * validate them against representative fixture objects. No live tool execution
 * is required — this is pure Zod schema validation.
 */
import { describe, expect, it } from 'vitest';
import { z } from 'zod';

import { LocalSearchCodeOutputSchema } from '../../src/tools/local_ripgrep/scheme.js';
import { LocalViewStructureOutputSchema } from '../../src/tools/local_view_structure/scheme.js';
import { LocalFindFilesOutputSchema } from '../../src/tools/local_find_files/scheme.js';
import { LocalGetFileContentOutputSchema } from '../../src/tools/local_fetch_content/scheme.js';
import { LocalBinaryInspectOutputSchema } from '../../src/tools/local_binary_inspect/scheme.js';
import {
  ItemPaginationSchema,
  CharPaginationSchema,
  ToolContinuationSchema,
  ToolDiagnosticSchema,
} from '../../src/scheme/pagination.js';

// ---------------------------------------------------------------------------
// Shared pagination schemas
// ---------------------------------------------------------------------------

describe('shared pagination schemas', () => {
  it('ItemPaginationSchema accepts canonical fields', () => {
    const result = ItemPaginationSchema.safeParse({
      currentPage: 1,
      totalPages: 3,
      hasMore: true,
      nextPage: 2,
      pageSize: 50,
      totalItems: 150,
    });
    expect(result.success).toBe(true);
  });

  it('ItemPaginationSchema rejects unknown alias fields in strict mode', () => {
    // Strict schema rejects unrecognized keys — aliases no longer exist.
    const withAliases = ItemPaginationSchema.strict().safeParse({
      currentPage: 1,
      totalPages: 1,
      hasMore: false,
      entriesPerPage: 50,  // old alias — no longer valid
      totalEntries: 150,   // old alias — no longer valid
    });
    expect(withAliases.success).toBe(false);
  });

  it('ItemPaginationSchema passes strict mode with only canonical fields', () => {
    const canonical = ItemPaginationSchema.strict().safeParse({
      currentPage: 1,
      totalPages: 1,
      hasMore: false,
    });
    expect(canonical.success).toBe(true);
  });

  it('ItemPaginationSchema requires currentPage/totalPages/hasMore', () => {
    expect(
      ItemPaginationSchema.safeParse({ hasMore: false }).success
    ).toBe(false);
    expect(
      ItemPaginationSchema.safeParse({ currentPage: 1, totalPages: 1, hasMore: false }).success
    ).toBe(true);
  });

  it('CharPaginationSchema accepts all fields', () => {
    const result = CharPaginationSchema.safeParse({
      charOffset: 0,
      charLength: 5000,
      totalChars: 15000,
      hasMore: true,
      nextCharOffset: 5000,
      currentPage: 1,
      totalPages: 3,
    });
    expect(result.success).toBe(true);
  });

  it('CharPaginationSchema requires the four core char fields', () => {
    expect(
      CharPaginationSchema.safeParse({ charOffset: 0, charLength: 100, totalChars: 200, hasMore: false }).success
    ).toBe(true);
    expect(
      CharPaginationSchema.safeParse({ charOffset: 0, hasMore: false }).success
    ).toBe(false);
  });

  it('ToolContinuationSchema requires tool and query', () => {
    expect(
      ToolContinuationSchema.safeParse({
        tool: 'localSearchCode',
        query: { path: '.', keywords: 'foo' },
        why: 'more results',
        confidence: 'exact',
      }).success
    ).toBe(true);
    expect(
      ToolContinuationSchema.safeParse({ tool: 'localSearchCode' }).success
    ).toBe(false);
  });

  it('ToolDiagnosticSchema validates level enum', () => {
    expect(
      ToolDiagnosticSchema.safeParse({ level: 'warning', message: 'too many files' }).success
    ).toBe(true);
    expect(
      ToolDiagnosticSchema.safeParse({ level: 'critical', message: 'x' }).success
    ).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// localSearchCode output schema
// ---------------------------------------------------------------------------

describe('LocalSearchCodeOutputSchema', () => {
  it('validates a typical paginated result', () => {
    const fixture = {
      results: [
        {
          id: 'q1',
          data: {
            files: [
              {
                path: 'src/index.ts',
                matches: [{ line: 10, value: 'export function runCLI' }],
                totalMatchRows: 1,
                returnedMatchRows: 1,
              },
            ],
            searchEngine: 'rg',
            pagination: {
              currentPage: 1,
              totalPages: 2,
              hasMore: true,
              nextPage: 2,
              pageSize: 20,
              totalItems: 35,
            },
            warnings: [],
          },
        },
      ],
      base: '/Users/dev/project',
    };
    const result = LocalSearchCodeOutputSchema.safeParse(fixture);
    expect(result.success, JSON.stringify(result)).toBe(true);
  });

  it('validates an empty result row', () => {
    const fixture = {
      results: [{ id: 'q1', status: 'empty', data: { files: [], warnings: [] } }],
    };
    expect(LocalSearchCodeOutputSchema.safeParse(fixture).success).toBe(true);
  });

  it('validates result with next continuation', () => {
    const fixture = {
      results: [
        {
          id: 'q1',
          data: {
            files: [],
            next: {
              nextPage: {
                tool: 'localSearchCode',
                query: { path: '.', keywords: 'foo', page: 2 },
              },
            },
          },
        },
      ],
    };
    expect(LocalSearchCodeOutputSchema.safeParse(fixture).success).toBe(true);
  });

  it('rejects results with missing id', () => {
    const fixture = {
      results: [{ data: { files: [] } }],
    };
    expect(LocalSearchCodeOutputSchema.safeParse(fixture).success).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// localViewStructure output schema
// ---------------------------------------------------------------------------

describe('LocalViewStructureOutputSchema', () => {
  it('validates a flat grouped result', () => {
    const fixture = {
      results: [
        {
          id: 'q1',
          data: {
            path: '/Users/dev/project/src',
            files: ['index.ts', 'config.ts'],
            folders: ['utils', 'tools'],
            summary: '42 entries (38 files, 4 dirs, 1.2MB)',
            pagination: {
              currentPage: 1,
              totalPages: 1,
              hasMore: false,
              pageSize: 100,
              totalItems: 42,
            },
          },
        },
      ],
    };
    expect(LocalViewStructureOutputSchema.safeParse(fixture).success).toBe(true);
  });

  it('validates a flat entries result', () => {
    const fixture = {
      results: [
        {
          id: 'q1',
          data: {
            entries: [
              { name: 'index.ts', type: 'file', depth: 1, size: '2.1KB' },
              { name: 'utils', type: 'dir', depth: 1 },
            ],
            pagination: {
              currentPage: 1,
              totalPages: 1,
              hasMore: false,
            },
          },
        },
      ],
    };
    expect(LocalViewStructureOutputSchema.safeParse(fixture).success).toBe(true);
  });

  it('rejects invalid entry type', () => {
    const fixture = {
      results: [
        {
          id: 'q1',
          data: {
            entries: [{ name: 'foo', type: 'block-device' }],
          },
        },
      ],
    };
    expect(LocalViewStructureOutputSchema.safeParse(fixture).success).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// localFindFiles output schema
// ---------------------------------------------------------------------------

describe('LocalFindFilesOutputSchema', () => {
  it('validates a typical find files result', () => {
    const fixture = {
      results: [
        {
          id: 'q1',
          data: {
            files: [
              { name: 'index.ts', path: '/project/src/index.ts', type: 'file', size: 2048 },
              { name: 'utils', path: '/project/src/utils', type: 'dir' },
            ],
            summary: '2 entries found',
            pagination: {
              currentPage: 1,
              totalPages: 1,
              hasMore: false,
              pageSize: 50,
              totalItems: 2,
            },
          },
        },
      ],
    };
    expect(LocalFindFilesOutputSchema.safeParse(fixture).success).toBe(true);
  });

  it('validates empty results', () => {
    const fixture = {
      results: [{ id: 'q1', status: 'empty', data: { files: [] } }],
    };
    expect(LocalFindFilesOutputSchema.safeParse(fixture).success).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// localGetFileContent output schema
// ---------------------------------------------------------------------------

describe('LocalGetFileContentOutputSchema', () => {
  it('validates a full file content result', () => {
    const fixture = {
      results: [
        {
          id: 'q1',
          data: {
            path: '/project/src/index.ts',
            content: 'export function main() {}',
            contentView: 'none',
            totalLines: 1,
            sourceChars: 25,
          },
        },
      ],
    };
    expect(LocalGetFileContentOutputSchema.safeParse(fixture).success).toBe(true);
  });

  it('validates a partial char-paginated result', () => {
    const fixture = {
      results: [
        {
          id: 'q1',
          data: {
            path: '/project/big.ts',
            content: 'export ...',
            contentView: 'standard',
            isPartial: true,
            totalLines: 1000,
            pagination: {
              charOffset: 0,
              charLength: 5000,
              totalChars: 50000,
              hasMore: true,
              nextCharOffset: 5000,
            },
          },
        },
      ],
    };
    expect(LocalGetFileContentOutputSchema.safeParse(fixture).success).toBe(true);
  });

  it('validates a match-range result', () => {
    const fixture = {
      results: [
        {
          id: 'q1',
          data: {
            path: '/project/src/index.ts',
            content: 'function foo() {}',
            matchRanges: [{ start: 10, end: 50 }],
            searchedFor: 'foo',
          },
        },
      ],
    };
    expect(LocalGetFileContentOutputSchema.safeParse(fixture).success).toBe(true);
  });

  it('validates a symbols/skeleton result', () => {
    const fixture = {
      results: [
        {
          id: 'q1',
          data: {
            path: '/project/src/big.ts',
            content: '1| export function foo() {}\n2| export function bar() {}',
            contentView: 'symbols',
            isSkeleton: true,
            totalLines: 500,
          },
        },
      ],
    };
    expect(LocalGetFileContentOutputSchema.safeParse(fixture).success).toBe(true);
  });

  it('rejects invalid contentView value', () => {
    const fixture = {
      results: [
        {
          id: 'q1',
          data: {
            contentView: 'full',
            content: 'x',
          },
        },
      ],
    };
    expect(LocalGetFileContentOutputSchema.safeParse(fixture).success).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// localBinaryInspect output schema
// ---------------------------------------------------------------------------

describe('LocalBinaryInspectOutputSchema', () => {
  it('validates an inspect mode result', () => {
    const fixture = {
      results: [
        {
          id: 'q1',
          data: {
            path: '/tmp/lib.dylib',
            mode: 'inspect',
            format: 'Mach-O',
            size: 524288,
            isText: false,
          },
        },
      ],
    };
    expect(LocalBinaryInspectOutputSchema.safeParse(fixture).success).toBe(true);
  });

  it('validates a list mode result with pagination', () => {
    const fixture = {
      results: [
        {
          id: 'q1',
          data: {
            path: '/tmp/archive.zip',
            mode: 'list',
            entries: [
              { name: 'README.md', size: 1024, isDir: false },
              { name: 'src/', isDir: true },
            ],
            pagination: {
              currentPage: 1,
              totalPages: 2,
              hasMore: true,
              nextPage: 2,
              pageSize: 25,
              totalItems: 42,
            },
          },
        },
      ],
    };
    expect(LocalBinaryInspectOutputSchema.safeParse(fixture).success).toBe(true);
  });

  it('validates an extract mode result with char pagination', () => {
    const fixture = {
      results: [
        {
          id: 'q1',
          data: {
            path: '/tmp/archive.tar.gz',
            mode: 'extract',
            archiveFile: 'README.md',
            content: '# My Project\n',
            isPartial: false,
          },
        },
      ],
    };
    expect(LocalBinaryInspectOutputSchema.safeParse(fixture).success).toBe(true);
  });

  it('validates a decompress mode result', () => {
    const fixture = {
      results: [
        {
          id: 'q1',
          data: {
            path: '/tmp/data.gz',
            mode: 'decompress',
            content: 'hello world',
            isPartial: false,
            pagination: {
              charOffset: 0,
              charLength: 11,
              totalChars: 11,
              hasMore: false,
            },
          },
        },
      ],
    };
    expect(LocalBinaryInspectOutputSchema.safeParse(fixture).success).toBe(true);
  });

  it('validates a strings mode result', () => {
    const fixture = {
      results: [
        {
          id: 'q1',
          data: {
            path: '/tmp/lib.so',
            mode: 'strings',
            strings: [
              { value: 'Hello, World!', offset: 1024 },
              { value: '/usr/local/lib' },
            ],
            totalStrings: 245,
            scanOffset: 0,
            nextScanOffset: 8192,
            hasMore: true,
          },
        },
      ],
    };
    expect(LocalBinaryInspectOutputSchema.safeParse(fixture).success).toBe(true);
  });

  it('validates an unpack mode result', () => {
    const fixture = {
      results: [
        {
          id: 'q1',
          data: {
            path: '/tmp/archive.zip',
            mode: 'unpack',
            localPath: '/Users/dev/.octocode/unpacked/archive-20260706',
            fileCount: 42,
            totalSize: 1048576,
          },
        },
      ],
    };
    expect(LocalBinaryInspectOutputSchema.safeParse(fixture).success).toBe(true);
  });

  it('validates error status row', () => {
    const fixture = {
      results: [
        {
          id: 'q1',
          status: 'error',
          data: {
            path: '/tmp/bad.zip',
            mode: 'list',
          },
        },
      ],
    };
    expect(LocalBinaryInspectOutputSchema.safeParse(fixture).success).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Schema JSON-schema derivation smoke test
// ---------------------------------------------------------------------------

describe('output schemas can generate JSON schema', () => {
  it.each([
    ['LocalSearchCodeOutputSchema', LocalSearchCodeOutputSchema],
    ['LocalViewStructureOutputSchema', LocalViewStructureOutputSchema],
    ['LocalFindFilesOutputSchema', LocalFindFilesOutputSchema],
    ['LocalGetFileContentOutputSchema', LocalGetFileContentOutputSchema],
  ])('%s can be converted to JSON Schema without error', (_name, schema) => {
    expect(() => z.toJSONSchema(schema)).not.toThrow();
  });
});
