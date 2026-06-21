import { describe, it, expect } from 'vitest';
import {
  processFileContentAPI,
  applyContentPagination,
} from '../../../octocode-tools-core/src/github/fileContentProcess.js';

const TS_CONTENT = [
  'import { useState } from "react";',
  '',
  '// Top-level comment that should be stripped',
  'export function Counter() {',
  '  const [count, setCount] = useState(0); // inline counter init',
  '  // increment handler',
  '  const increment = () => setCount(c => c + 1);',
  '  return count; // return the current value',
  '}',
].join('\n');

describe('processFileContentAPI — minify mode', () => {
  it('strips comments by default (minify omitted → inherits "standard" from config)', async () => {
    const result = await processFileContentAPI(
      TS_CONTENT,
      'facebook',
      'react',
      'main',
      'src/Counter.ts',
      true
    );
    expect(result.content).not.toContain('// Top-level comment');
    expect(result.content).not.toContain('// inline counter init');
    expect(result.content).toContain('export function Counter');
  });

  it('preserves comments with explicit minify:"none"', async () => {
    const result = await processFileContentAPI(
      TS_CONTENT,
      'facebook',
      'react',
      'main',
      'src/Counter.ts',
      true,
      undefined,
      undefined,
      5,
      undefined,
      undefined,
      undefined,
      'none'
    );
    expect(result.content).toContain(
      '// Top-level comment that should be stripped'
    );
    expect(result.content).toContain('// inline counter init');
  });

  it('strips comments with minify:"standard"', async () => {
    const result = await processFileContentAPI(
      TS_CONTENT,
      'facebook',
      'react',
      'main',
      'src/Counter.ts',
      true,
      undefined,
      undefined,
      5,
      undefined,
      undefined,
      undefined,
      'standard'
    );
    expect(result.content).not.toContain('// Top-level comment');
    expect(result.content).not.toContain('// inline counter init');
    expect(result.content).not.toContain('// increment handler');
    expect(result.content).toContain('export function Counter');
  });

  it('minify:"symbols" returns the skeleton with bodies dropped', async () => {
    const result = await processFileContentAPI(
      TS_CONTENT,
      'facebook',
      'react',
      'main',
      'src/Counter.ts',
      false,
      undefined,
      undefined,
      5,
      undefined,
      undefined,
      undefined,
      'symbols'
    );
    expect(result.signaturesExtracted).toBe(true);
    expect(result.contentView).toBe('symbols');
    expect(result.isSkeleton).toBe(true);
    expect(result.isPartial).toBe(false);
    expect(result.content).toContain('import { useState }');
    expect(result.content).toContain('export function Counter');
    expect(result.content).not.toContain('setCount(c => c + 1)');
    expect(result.sourceChars).toBe(TS_CONTENT.length);
    expect(result.sourceBytes).toBe(Buffer.byteLength(TS_CONTENT, 'utf-8'));
  });

  it('minify:"symbols" strips inline comments from import lines in the skeleton', async () => {
    const content = [
      'import { useState } from "react"; // state hook',
      'import { useEffect } from "react"; // effect hook',
      '',
      'export function Counter() {',
      '  return useState(0);',
      '}',
    ].join('\n');
    const result = await processFileContentAPI(
      content,
      'facebook',
      'react',
      'main',
      'src/Counter.ts',
      false,
      undefined,
      undefined,
      5,
      undefined,
      undefined,
      undefined,
      'symbols'
    );
    expect(result.content).not.toContain('// state hook');
    expect(result.content).not.toContain('// effect hook');
    expect(result.content).toContain('import { useState }');
    expect(result.content).toContain('export function Counter');
  });

  it('minify:"symbols" keeps the shebang in a shell skeleton', async () => {
    const shContent = [
      '#!/usr/bin/env bash',
      '',
      'greet() {',
      '  echo "hi" # inline comment',
      '}',
    ].join('\n');
    const result = await processFileContentAPI(
      shContent,
      'nvm-sh',
      'nvm',
      'main',
      'install.sh',
      false,
      undefined,
      undefined,
      5,
      undefined,
      undefined,
      undefined,
      'symbols'
    );
    expect(result.signaturesExtracted).toBe(true);
    expect(result.content).toContain('#!/usr/bin/env bash');
    expect(result.content).toContain('greet() {');
    expect(result.content).not.toContain('echo "hi"');
  });

  it('minify:"symbols" on an unsupported file type warns and falls back to standard content view', async () => {
    const txt = 'name=octocode\n\n\n; token-saving comment\nkeep=true\n';
    const result = await processFileContentAPI(
      txt,
      'o',
      'r',
      'main',
      'settings.ini',
      false,
      undefined,
      undefined,
      5,
      undefined,
      undefined,
      undefined,
      'symbols'
    );
    expect(result.signaturesExtracted).toBeUndefined();
    expect(result.contentView).toBeUndefined();
    expect(result.isSkeleton).toBeUndefined();
    expect(result.content).toContain('name=octocode');
    expect(result.content).toContain('keep=true');
    expect(result.content).not.toContain('; token-saving comment');
    expect(
      result.warnings?.some(w => w.includes('not supported for this file type'))
    ).toBe(true);
    expect(
      result.warnings?.some(w =>
        w.includes('falling back to standard content view')
      )
    ).toBe(true);
    expect(result.warnings?.join('\n')).not.toContain('returning full content');
  });

  it('minify:"symbols" on markdown returns a heading outline', async () => {
    const result = await processFileContentAPI(
      [
        '# Readme',
        '',
        'Intro prose that should not be returned.',
        '~~~',
        '## Not a heading',
        '~~~',
        '## Usage',
      ].join('\n'),
      'octocode',
      'repo',
      'main',
      'README.md',
      false,
      undefined,
      undefined,
      5,
      undefined,
      undefined,
      undefined,
      'symbols'
    );

    expect(result.signaturesExtracted).toBe(true);
    expect(result.contentView).toBe('symbols');
    expect(result.isSkeleton).toBe(true);
    expect(result.content).toContain('   1| # Readme');
    expect(result.content).toContain('   7|   ## Usage');
    expect(result.content).not.toContain('Intro prose');
    expect(result.content).not.toContain('Not a heading');
    expect(result.warnings).toBeUndefined();
  });
});

describe('applyContentPagination — chars mode (not bytes)', () => {
  it('does NOT paginate when charCount <= limit even if byteCount > limit', () => {
    const cjk = '中';
    const content = cjk.repeat(50);
    expect(content.length).toBe(50); // 50 JS chars
    expect(Buffer.byteLength(content, 'utf-8')).toBeGreaterThan(100); // 150 bytes

    const data = {
      owner: 'test',
      repo: 'repo',
      path: 'file.ts',
      content,
      branch: 'main',
      totalLines: 1,
    };

    const result = applyContentPagination(data, 0, 100);
    expect(result.content).toBe(content);
    expect(result.pagination).toBeUndefined();
  });

  it('paginates when charCount exceeds limit', () => {
    const content = 'a'.repeat(200);

    const data = {
      owner: 'test',
      repo: 'repo',
      path: 'file.ts',
      content,
      branch: 'main',
      totalLines: 1,
    };

    const result = applyContentPagination(data, 0, 100);
    expect(result.content).toHaveLength(100);
    expect(result.pagination?.hasMore).toBe(true);
  });

  it('pagination output contains only char fields — no byteOffset/byteLength/totalBytes', () => {
    const content = 'x'.repeat(200);
    const data = {
      owner: 'test',
      repo: 'repo',
      path: 'file.ts',
      content,
      branch: 'main',
      totalLines: 1,
    };

    const result = applyContentPagination(data, 0, 100);
    expect(result.pagination).toBeDefined();
    expect(result.pagination).not.toHaveProperty('byteOffset');
    expect(result.pagination).not.toHaveProperty('byteLength');
    expect(result.pagination).not.toHaveProperty('totalBytes');
    expect(result.pagination).toHaveProperty('charOffset');
    expect(result.pagination).toHaveProperty('charLength');
    expect(result.pagination).toHaveProperty('totalChars');
  });

  it('charOffset advances by chars, not bytes', () => {
    const cjk = '中'.repeat(30); // 30 chars, 90 bytes
    const ascii = 'x'.repeat(100);
    const content = cjk + ascii;

    const data = {
      owner: 'test',
      repo: 'repo',
      path: 'file.ts',
      content,
      branch: 'main',
      totalLines: 1,
    };

    const result = applyContentPagination(data, 30, 100);
    expect(result.content).toBe(ascii.slice(0, 100));
  });
});
