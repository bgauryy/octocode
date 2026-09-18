import { describe, expect, it } from 'vitest';
import {
  paginateBulkText,
  appendResponsePagination,
} from '../../src/utils/response/bulk/pagination.js';

// ---------------------------------------------------------------------------
// paginateBulkText
// ---------------------------------------------------------------------------

describe('paginateBulkText', () => {
  it('returns the full text unchanged when no pagination is specified', () => {
    const result = paginateBulkText('hello world');
    expect(result.text).toBe('hello world');
    expect(result.pagination).toBeUndefined();
  });

  it('returns the full text unchanged when responseCharLength is undefined', () => {
    const result = paginateBulkText('hello world', {});
    expect(result.text).toBe('hello world');
    expect(result.pagination).toBeUndefined();
  });

  it('paginates text when responseCharLength is set', () => {
    const text = 'line1\nline2\nline3\nline4\nline5\n';
    const result = paginateBulkText(text, { responseCharLength: 12 });
    expect(result.pagination).toBeDefined();
    expect(result.pagination!.currentPage).toBe(1);
    expect(result.pagination!.hasMore).toBe(true);
  });

  it('includes a page banner in the text', () => {
    const text =
      'hello world, this is a longer piece of text for testing pagination';
    const result = paginateBulkText(text, { responseCharLength: 20 });
    expect(result.text).toMatch(/# Response page \d+\/\d+/);
  });

  it('returns hasMore=false and no nextCharOffset on last page', () => {
    const text = 'short';
    const result = paginateBulkText(text, { responseCharLength: 100 });
    expect(result.pagination!.hasMore).toBe(false);
    expect(result.pagination!.nextCharOffset).toBeUndefined();
  });

  it('includes nextCharOffset in pagination when hasMore=true', () => {
    const text = 'a'.repeat(100);
    const result = paginateBulkText(text, { responseCharLength: 30 });
    if (result.pagination!.hasMore) {
      expect(result.pagination!.nextCharOffset).toBeDefined();
    }
  });

  it('respects responseCharOffset to start reading from a position', () => {
    const text = 'aaaa\nbbbb\ncccc\ndddd\n';
    const first = paginateBulkText(text, { responseCharLength: 10 });
    const result = paginateBulkText(text, {
      responseCharLength: 10,
      responseCharOffset: 5,
      responseSnapshot: first.pagination!.snapshot,
    });
    expect(result.pagination!.charOffset).toBe(5);
  });

  it('requires a snapshot for later pages', () => {
    const result = paginateBulkText('a'.repeat(100), {
      responseCharLength: 10,
      responseCharOffset: 10,
    });
    expect(result.pagination).toMatchObject({
      changed: false,
      restart: true,
      charLength: 0,
      nextCharOffset: 0,
    });
    expect(result.text).toContain('restart');
  });

  it('restarts when the whole response snapshot changes', () => {
    const first = paginateBulkText('first response', { responseCharLength: 5 });
    const result = paginateBulkText('changed response', {
      responseCharLength: 5,
      responseCharOffset: 5,
      responseSnapshot: first.pagination!.snapshot,
    });
    expect(result.pagination).toMatchObject({
      changed: true,
      restart: true,
      expectedSnapshot: first.pagination!.snapshot,
      nextCharOffset: 0,
    });
    expect(result.text).not.toContain('first response');
  });

  it('restarts an offset beyond the changed response instead of accepting the clamp', () => {
    const result = paginateBulkText('short', {
      responseCharLength: 5,
      responseCharOffset: 500,
      responseSnapshot: 'response-v1:stale',
    });
    expect(result.pagination).toMatchObject({
      restart: true,
      changed: true,
      charLength: 0,
      nextCharOffset: 0,
    });
  });

  it('handles an empty string', () => {
    const result = paginateBulkText('', { responseCharLength: 100 });
    expect(result.pagination).toBeDefined();
    expect(result.pagination!.totalChars).toBe(0);
    expect(result.pagination!.hasMore).toBe(false);
  });

  it('restarts a manually selected offset inside a Unicode code point', () => {
    const text = 'a😀b';
    const first = paginateBulkText(text, { responseCharLength: 1 });
    const result = paginateBulkText(text, {
      responseCharLength: 1,
      responseCharOffset: 2,
      responseSnapshot: first.pagination!.snapshot,
    });
    expect(result.pagination).toMatchObject({
      restart: true,
      changed: false,
      charLength: 0,
      nextCharOffset: 0,
    });
    expect(result.text).toContain('Unicode');
  });

  it('snaps to a newline boundary when possible', () => {
    // 20-char page on text with a \n at position 10
    const text = '0123456789\n0123456789abc';
    const result = paginateBulkText(text, { responseCharLength: 15 });
    const pageContent = result.text.replace(/# Response page.*\n/, '');
    // Should snap to after the \n at position 11
    expect(pageContent).toBe('0123456789\n');
    expect(result.pagination!.nextCharOffset).toBe(11);
  });

  it.each([1, 2, 3, 7])(
    'preserves Unicode through separately encoded pages of length %i',
    length => {
      const text = 'a😀b\n🧪研究c😀';
      let offset = 0;
      let snapshot: string | undefined;
      const pages: string[] = [];
      do {
        const result = paginateBulkText(text, {
          responseCharLength: length,
          responseCharOffset: offset,
          responseSnapshot: snapshot,
        });
        const chunk = result.text.slice(result.text.indexOf('\n') + 1);
        pages.push(Buffer.from(chunk, 'utf8').toString('utf8'));
        snapshot = result.pagination!.snapshot;
        if (!result.pagination!.hasMore) break;
        expect(result.pagination!.nextCharOffset).toBeGreaterThan(offset);
        offset = result.pagination!.nextCharOffset!;
        expect(pages.length).toBeLessThanOrEqual(text.length);
      } while (true);
      expect(pages.join('')).toBe(text);
    }
  );

  it('calculates totalPages >= currentPage', () => {
    const text = 'x'.repeat(200);
    const result = paginateBulkText(text, { responseCharLength: 50 });
    expect(result.pagination!.totalPages).toBeGreaterThanOrEqual(
      result.pagination!.currentPage
    );
  });

  it('totalChars matches text length', () => {
    const text = 'hello world';
    const result = paginateBulkText(text, { responseCharLength: 5 });
    expect(result.pagination!.totalChars).toBe(text.length);
    expect(result.pagination!.snapshot).toMatch(/^response-v1:/);
  });
});

// ---------------------------------------------------------------------------
// appendResponsePagination
// ---------------------------------------------------------------------------

describe('appendResponsePagination', () => {
  it('returns the structured content unchanged when pagination is undefined', () => {
    const content = { results: [] };
    expect(appendResponsePagination(content, undefined)).toBe(content);
  });

  it('appends responsePagination to structured content', () => {
    const content = { results: ['a', 'b'] };
    const pagination = {
      currentPage: 1,
      totalPages: 3,
      hasMore: true,
      charOffset: 0,
      charLength: 50,
      totalChars: 150,
      nextCharOffset: 50,
    };
    const result = appendResponsePagination(content, pagination);
    expect(result.responsePagination).toEqual(pagination);
    expect(result.results).toEqual(['a', 'b']);
  });

  it('does not mutate the original content', () => {
    const original = { data: 'x' };
    appendResponsePagination(original, {
      currentPage: 1,
      totalPages: 2,
      hasMore: true,
      charOffset: 0,
      charLength: 10,
      totalChars: 20,
    });
    expect(original).not.toHaveProperty('responsePagination');
  });
});
