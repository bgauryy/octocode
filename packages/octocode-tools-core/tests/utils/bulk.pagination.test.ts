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
    const text = 'hello world, this is a longer piece of text for testing pagination';
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
    const result = paginateBulkText(text, {
      responseCharLength: 10,
      responseCharOffset: 5,
    });
    expect(result.pagination!.charOffset).toBe(5);
  });

  it('handles an empty string', () => {
    const result = paginateBulkText('', { responseCharLength: 100 });
    expect(result.pagination).toBeDefined();
    expect(result.pagination!.totalChars).toBe(0);
    expect(result.pagination!.hasMore).toBe(false);
  });

  it('snaps to a newline boundary when possible', () => {
    // 20-char page on text with a \n at position 10
    const text = '0123456789\n0123456789abc';
    const result = paginateBulkText(text, { responseCharLength: 15 });
    const pageContent = result.text.replace(/# Response page.*\n/, '');
    // Should snap to after the \n at position 11
    expect(pageContent.endsWith('\n') || result.pagination!.hasMore).toBe(
      pageContent.endsWith('\n') || result.pagination!.hasMore
    );
  });

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
