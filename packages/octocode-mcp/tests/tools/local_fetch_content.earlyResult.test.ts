import { describe, it, expect, beforeEach, vi } from 'vitest';
import { fetchContent } from '../../../octocode-tools-core/src/tools/local_fetch_content/fetchContent.js';
import * as fs from 'fs/promises';
import * as pathValidator from 'octocode-security/pathValidator';

vi.mock('fs/promises', () => ({
  open: vi.fn(),
  readFile: vi.fn(),
  stat: vi.fn(),
}));

vi.mock('octocode-security/pathValidator', () => ({
  pathValidator: {
    validate: vi.fn(),
  },
}));

function buildHugeCommentedContent(lineCount = 500): string {
  return Array.from(
    { length: lineCount },
    (_, i) =>
      `const EARLYMARKER_${String(i).padStart(3, '0')} = ${i}; // inline comment ${i}`
  ).join('\n');
}

describe('fetchContent — earlyResult minification path', () => {
  const mockOpen = vi.mocked(fs.open);
  const mockReadFile = vi.mocked(fs.readFile);
  const mockStat = vi.mocked(fs.stat);
  const mockValidate = vi.mocked(pathValidator.pathValidator.validate);

  beforeEach(() => {
    vi.clearAllMocks();
    mockValidate.mockReturnValue({ isValid: true });
    mockStat.mockResolvedValue({ size: 1024 } as unknown as Awaited<
      ReturnType<typeof fs.stat>
    >);
    mockOpen.mockResolvedValue({
      read: vi.fn().mockResolvedValue({ bytesRead: 0 }),
      close: vi.fn().mockResolvedValue(undefined),
    } as unknown as Awaited<ReturnType<typeof fs.open>>);
  });

  it('earlyResult is triggered when matchString result exceeds the output budget', async () => {
    const content = buildHugeCommentedContent(400);
    expect(content.length).toBeGreaterThan(8000);
    mockReadFile.mockResolvedValue(content);

    const result = await fetchContent({
      path: 'huge.ts',
      matchString: 'EARLYMARKER',
      contextLines: 0,
    });

    expect(result.pagination).toBeDefined();
    expect(result.pagination?.hasMore).toBe(true);
  });

  it('minify:"standard" strips inline comments from the earlyResult slice', async () => {
    const content = buildHugeCommentedContent();
    mockReadFile.mockResolvedValue(content);

    const result = await fetchContent({
      path: 'huge.ts',
      matchString: 'EARLYMARKER',
      contextLines: 0,
      minify: 'standard',
    });

    expect(result.pagination?.hasMore).toBe(true);
    expect(result.content).not.toContain('// inline comment');
    expect(result.content).toContain('EARLYMARKER');
  });

  it('default (minify omitted) preserves inline comments in the earlyResult slice', async () => {
    const content = buildHugeCommentedContent(400);
    mockReadFile.mockResolvedValue(content);

    const result = await fetchContent({
      path: 'huge.ts',
      matchString: 'EARLYMARKER',
      contextLines: 0,
    });

    expect(result.pagination?.hasMore).toBe(true);
    expect(result.content).toContain('// inline comment');
    expect(result.content).toContain('EARLYMARKER');
  });

  it('minify:"standard" and minify:"none" produce different content on earlyResult path', async () => {
    const content = buildHugeCommentedContent(400);
    mockReadFile.mockResolvedValue(content);

    const withMinify = await fetchContent({
      path: 'huge.ts',
      matchString: 'EARLYMARKER',
      contextLines: 0,
      minify: 'standard',
    });

    mockReadFile.mockResolvedValue(content);

    const withoutMinify = await fetchContent({
      path: 'huge.ts',
      matchString: 'EARLYMARKER',
      contextLines: 0,
      minify: 'none',
    });

    expect(withMinify.content).not.toBe(withoutMinify.content);
    expect(withMinify.content).not.toContain('// inline comment');
    expect(withoutMinify.content).toContain('// inline comment');
  });
});
