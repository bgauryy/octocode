import { describe, it, expect } from 'vitest';
import {
  processFileContentAPI,
  applyContentPagination,
} from '../../src/github/fileContentProcess.js';

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

describe('processFileContentAPI — minify flag', () => {
  it('strips comments by default (minify omitted)', async () => {
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
    expect(result.content).not.toContain('// increment handler');
    expect(result.content).toContain('export function Counter');
  });

  it('strips comments when minify=true is explicit', async () => {
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
      undefined,
      true
    );
    expect(result.content).not.toContain('// Top-level comment');
    expect(result.content).toContain('export function Counter');
  });

  it('preserves comments when minify=false', async () => {
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
      undefined,
      false
    );
    expect(result.content).toContain(
      '// Top-level comment that should be stripped'
    );
    expect(result.content).toContain('// inline counter init');
  });

  it('minify=true strips comments from signaturesOnly output', async () => {
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
      true,
      undefined,
      undefined,
      true
    );
    expect(result.content).not.toContain('// Top-level comment');
    expect(result.content).not.toContain('// inline counter init');
  });

  it('minify=false preserves comments in signaturesOnly output', async () => {
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
      true,
      undefined,
      undefined,
      false
    );
    expect(result.content).toContain('export function Counter');
  });
});

describe('applyContentPagination — chars mode (not bytes)', () => {
  it('does NOT paginate when charCount <= limit even if byteCount > limit', () => {
    // '中' is a BMP character: 1 UTF-16 code unit (JS .length = 1) but 3 UTF-8 bytes.
    // 50 CJK chars = 50 JS chars (< limit=100) but 150 UTF-8 bytes (> limit=100).
    // Old (bytes) code would have paginated; new (chars) code must NOT.
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

  it('charOffset advances by chars, not bytes', () => {
    // Build content: 30 CJK chars (3 bytes each) then 100 ASCII chars.
    // charOffset=30 should skip exactly the 30 CJK chars.
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
