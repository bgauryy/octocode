import { describe, it, expect, vi } from 'vitest';
import { ResearchResponse, detectLanguageFromPath } from '../../utils/responseBuilder.js';

vi.mock('octocode-mcp/public', () => ({
  createRoleBasedResult: vi.fn((opts) => ({
    content: [
      { type: 'text', text: opts.assistant?.summary || '' },
    ],
    structuredContent: opts.data,
    isError: opts.isError || false,
  })),
  QuickResult: {
    success: vi.fn((summary, data, hints) => ({
      content: [{ type: 'text', text: summary }],
      structuredContent: data,
      hints,
      isError: false,
    })),
    empty: vi.fn((summary, hints) => ({
      content: [{ type: 'text', text: summary }],
      hints,
      isEmpty: true,
    })),
    paginated: vi.fn((summary, data, pagination, hints) => ({
      content: [{ type: 'text', text: summary }],
      structuredContent: data,
      pagination,
      hints,
    })),
  },
  StatusEmoji: {
    success: '✅',
    error: '❌',
    empty: '📭',
    file: '📄',
    folder: '📁',
    definition: '📍',
    reference: '🔗',
    call: '📞',
    info: 'ℹ️',
    partial: '⚠️',
  },
  ContentBuilder: vi.fn(),
}));

describe('detectLanguageFromPath', () => {
  it('detects TypeScript files', () => {
    expect(detectLanguageFromPath('src/utils.ts')).toBe('typescript');
    expect(detectLanguageFromPath('components/App.tsx')).toBe('typescript');
  });

  it('detects JavaScript files', () => {
    expect(detectLanguageFromPath('index.js')).toBe('javascript');
    expect(detectLanguageFromPath('components/Button.jsx')).toBe('javascript');
  });

  it('detects Python files', () => {
    expect(detectLanguageFromPath('main.py')).toBe('python');
  });

  it('detects Go files', () => {
    expect(detectLanguageFromPath('main.go')).toBe('go');
  });

  it('detects Rust files', () => {
    expect(detectLanguageFromPath('lib.rs')).toBe('rust');
  });

  it('detects Java files', () => {
    expect(detectLanguageFromPath('Main.java')).toBe('java');
  });

  it('detects Ruby files', () => {
    expect(detectLanguageFromPath('app.rb')).toBe('ruby');
  });

  it('detects shell scripts', () => {
    expect(detectLanguageFromPath('install.sh')).toBe('bash');
    expect(detectLanguageFromPath('run.bash')).toBe('bash');
  });

  it('detects config files', () => {
    expect(detectLanguageFromPath('config.yml')).toBe('yaml');
    expect(detectLanguageFromPath('config.yaml')).toBe('yaml');
    expect(detectLanguageFromPath('package.json')).toBe('json');
  });

  it('detects web files', () => {
    expect(detectLanguageFromPath('index.html')).toBe('html');
    expect(detectLanguageFromPath('styles.css')).toBe('css');
    expect(detectLanguageFromPath('styles.scss')).toBe('scss');
  });

  it('returns empty string for unknown extensions', () => {
    expect(detectLanguageFromPath('data.xyz')).toBe('');
    expect(detectLanguageFromPath('README')).toBe('');
  });

  it('handles nested paths', () => {
    expect(detectLanguageFromPath('/Users/dev/project/src/index.ts')).toBe('typescript');
    expect(detectLanguageFromPath('a/b/c/d/e.py')).toBe('python');
  });

  it('handles uppercase extensions', () => {
    expect(detectLanguageFromPath('file.TS')).toBe('typescript');
    expect(detectLanguageFromPath('file.PY')).toBe('python');
  });
});

describe('ResearchResponse.searchResults', () => {
  it('formats results with files', () => {
    const result = ResearchResponse.searchResults({
      files: [
        { path: 'src/utils.ts', matches: 3, line: 10 },
        { path: 'src/index.ts', matches: 1, line: 5 },
      ],
      totalMatches: 4,
      searchPattern: 'function',
    });

    expect(result.content[0].text).toContain('Found 4 matches');
    expect(result.content[0].text).toContain('function');
    expect(result.content[0].text).toContain('src/utils.ts');
    expect(result.content[0].text).toContain('src/index.ts');
  });

  it('handles empty results', () => {
    const result = ResearchResponse.searchResults({
      files: [],
      totalMatches: 0,
      searchPattern: 'nonexistent',
    });

    expect(result.content[0].text).toContain('No matches found');
    expect(result.content[0].text).toContain('nonexistent');
    expect(result.isEmpty).toBe(true);
  });

  it('truncates file list at 10 files', () => {
    const files = Array.from({ length: 15 }, (_, i) => ({
      path: `file${i}.ts`,
      matches: 1,
    }));

    const result = ResearchResponse.searchResults({
      files,
      totalMatches: 15,
    });

    expect(result.content[0].text).toContain('and 5 more files');
  });

  it('includes preview text when available', () => {
    const result = ResearchResponse.searchResults({
      files: [
        { path: 'src/utils.ts', matches: 1, preview: 'export function helper() {' },
      ],
      totalMatches: 1,
    });

    expect(result.content[0].text).toContain('export function helper()');
  });

  it('handles non-string preview gracefully (no crash)', () => {
    const result = ResearchResponse.searchResults({
      files: [
        { path: 'src/utils.ts', matches: 1, preview: 42 as unknown as string },
      ],
      totalMatches: 1,
    });

    expect(result.content[0].text).toContain('src/utils.ts');
    expect(result.content[0].text).not.toContain('.slice');
  });

  it('handles undefined preview gracefully', () => {
    const result = ResearchResponse.searchResults({
      files: [
        { path: 'src/utils.ts', matches: 1 },
      ],
      totalMatches: 1,
    });

    expect(result.content[0].text).toContain('src/utils.ts');
  });

  it('includes repo info for GitHub results', () => {
    const result = ResearchResponse.searchResults({
      files: [
        { path: 'src/index.ts', repo: 'anthropic/claude', matches: 1 },
      ],
      totalMatches: 1,
      isLocal: false,
    });

    expect(result.content[0].text).toContain('anthropic/claude');
  });

  it('adds pagination hints when hasMore', () => {
    const result = ResearchResponse.searchResults({
      files: [{ path: 'file.ts', matches: 1 }],
      totalMatches: 100,
      pagination: { page: 1, total: 10, hasMore: true },
    });

    expect(result.hints).toContain('Next page: page=2');
  });

  it('passes through MCP hints', () => {
    const result = ResearchResponse.searchResults({
      files: [{ path: 'file.ts', matches: 1 }],
      totalMatches: 1,
      mcpHints: ['Use localGetFileContent to read full file'],
    });

    expect(result.hints).toContain('Use localGetFileContent to read full file');
  });
});

describe('ResearchResponse.fileContent', () => {
  it('formats content with code fence', () => {
    const result = ResearchResponse.fileContent({
      path: 'src/utils.ts',
      content: 'export function helper() {}',
    });

    expect(result.content[0].text).toContain('📄 src/utils.ts');
    expect(result.content[0].text).toContain('```typescript');
    expect(result.content[0].text).toContain('export function helper()');
    expect(result.content[0].text).toContain('```');
  });

  it('includes line range info', () => {
    const result = ResearchResponse.fileContent({
      path: 'src/utils.ts',
      content: 'line 10 content',
      lines: { start: 10, end: 20 },
    });

    expect(result.content[0].text).toContain('lines 10-20');
  });

  it('uses explicit language when provided', () => {
    const result = ResearchResponse.fileContent({
      path: 'Dockerfile',
      content: 'FROM node:18',
      language: 'dockerfile',
    });

    expect(result.content[0].text).toContain('```dockerfile');
  });

  it('adds hints for partial content', () => {
    const result = ResearchResponse.fileContent({
      path: 'src/large.ts',
      content: 'partial...',
      isPartial: true,
      totalLines: 1000,
    });

    expect(result.structuredContent.isPartial).toBe(true);
  });
});

describe('ResearchResponse.lspResult', () => {
  it('formats definition results', () => {
    const result = ResearchResponse.lspResult({
      symbol: 'myFunction',
      locations: [
        { uri: 'file:///src/utils.ts', line: 10, preview: 'function myFunction() {' },
      ],
      type: 'definition',
    });

    expect(result.content[0].text).toContain('Definition for "myFunction"');
    expect(result.content[0].text).toContain('utils.ts:10');
  });

  it('formats reference results', () => {
    const result = ResearchResponse.lspResult({
      symbol: 'myFunction',
      locations: [
        { uri: 'file:///src/index.ts', line: 5 },
        { uri: 'file:///src/app.ts', line: 20 },
      ],
      type: 'references',
    });

    expect(result.content[0].text).toContain('References for "myFunction"');
    expect(result.content[0].text).toContain('index.ts:5');
    expect(result.content[0].text).toContain('app.ts:20');
  });

  it('handles empty locations', () => {
    const result = ResearchResponse.lspResult({
      symbol: 'unknownSymbol',
      locations: [],
      type: 'definition',
    });

    expect(result.content[0].text).toContain('No definition found');
    expect(result.content[0].text).toContain('unknownSymbol');
  });

  it('adds appropriate follow-up hints', () => {
    const result = ResearchResponse.lspResult({
      symbol: 'fn',
      locations: [{ uri: 'file:///src/utils.ts', line: 1 }],
      type: 'definition',
    });

    expect(result.structuredContent.type).toBe('definition');
  });
});

describe('ResearchResponse.repoStructure', () => {
  it('formats directory structure', () => {
    const result = ResearchResponse.repoStructure({
      path: '/src',
      structure: {
        files: ['index.ts', 'utils.ts'],
        folders: ['components', 'hooks'],
      },
    });

    expect(result.content[0].text).toContain('📁 /src');
    expect(result.content[0].text).toContain('📁 components');
    expect(result.content[0].text).toContain('📄 index.ts');
  });

  it('includes owner/repo for GitHub repos', () => {
    const result = ResearchResponse.repoStructure({
      path: '',
      structure: { files: ['README.md'], folders: ['src'] },
      owner: 'anthropic',
      repo: 'claude',
    });

    expect(result.content[0].text).toContain('anthropic/claude');
  });

  it('truncates long lists', () => {
    const files = Array.from({ length: 25 }, (_, i) => `file${i}.ts`);
    const result = ResearchResponse.repoStructure({
      path: '/',
      structure: { files, folders: [] },
    });

    expect(result.content[0].text).not.toContain('file24.ts');
  });
});

describe('ResearchResponse.packageSearch', () => {
  it('formats npm package results', () => {
    const result = ResearchResponse.packageSearch({
      packages: [
        { name: 'express', version: '4.18.2', description: 'Fast web framework' },
        { name: 'koa', version: '2.14.1', description: 'Expressive middleware' },
      ],
      registry: 'npm',
      query: 'web framework',
    });

    expect(result.content[0].text).toContain('Found 2 packages');
    expect(result.content[0].text).toContain('NPM');
    expect(result.content[0].text).toContain('express@4.18.2');
  });

  it('handles empty results', () => {
    const result = ResearchResponse.packageSearch({
      packages: [],
      registry: 'pypi',
      query: 'nonexistent-package',
    });

    expect(result.content[0].text).toContain('No packages found');
    expect(result.isEmpty).toBe(true);
  });

  it('handles non-string description gracefully', () => {
    const result = ResearchResponse.packageSearch({
      packages: [
        { name: 'test-pkg', version: '1.0.0', description: undefined },
      ],
      registry: 'npm',
    });

    expect(result.content[0].text).toContain('No description');
  });

  it('handles numeric description gracefully', () => {
    const result = ResearchResponse.packageSearch({
      packages: [
        { name: 'test-pkg', version: '1.0.0', description: 123 as unknown as string },
      ],
      registry: 'npm',
    });

    expect(result.content[0].text).toContain('No description');
  });
});

describe('ResearchResponse.bulkResult', () => {
  it('formats bulk operation summary', () => {
    const result = ResearchResponse.bulkResult({
      results: [
        { status: 'success', data: {} },
        { status: 'success', data: {} },
        { status: 'error', error: 'Failed' },
      ],
      operation: 'search',
      totalQueries: 3,
    });

    expect(result.content[0].text).toContain('Bulk search completed');
    expect(result.content[0].text).toContain('Success: 2/3');
    expect(result.content[0].text).toContain('Failed: 1/3');
  });

  it('handles all success', () => {
    const result = ResearchResponse.bulkResult({
      results: [
        { status: 'success', data: {} },
        { status: 'success', data: {} },
      ],
      operation: 'fetch',
      totalQueries: 2,
    });

    expect(result.isError).toBe(false);
  });

  it('marks as error when all fail', () => {
    const result = ResearchResponse.bulkResult({
      results: [
        { status: 'error', error: 'Failed' },
        { status: 'error', error: 'Failed' },
      ],
      operation: 'search',
      totalQueries: 2,
    });

    expect(result.isError).toBe(true);
  });

  it('includes empty count when present', () => {
    const result = ResearchResponse.bulkResult({
      results: [
        { status: 'success', data: {} },
        { status: 'empty' },
      ],
      operation: 'search',
      totalQueries: 2,
    });

    expect(result.content[0].text).toContain('Empty: 1/2');
  });
});
