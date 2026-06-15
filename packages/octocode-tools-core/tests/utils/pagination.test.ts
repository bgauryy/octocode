import { describe, expect, it } from 'vitest';

import {
  applyPagination,
  createPaginationInfo,
  serializeForPagination,
} from '../../src/utils/pagination/core.js';
import { sliceContent } from '../../src/utils/file/byteOffset.js';

describe('pagination core', () => {
  it('returns full-content metadata when length is omitted', () => {
    const content = 'alpha\nbeta';
    const page = applyPagination(content);

    expect(page.paginatedContent).toBe(content);
    expect(page.hasMore).toBe(false);
    expect(page.currentPage).toBe(1);
    expect(page.totalPages).toBe(1);
    expect(page.byteLength).toBe(Buffer.byteLength(content, 'utf8'));
    expect(page.charLength).toBe(content.length);
  });

  it('paginates by characters and reports deterministic continuation offsets', () => {
    const page = applyPagination('0123456789', 3, 4);

    expect(page.paginatedContent).toBe('3456');
    expect(page.charOffset).toBe(3);
    expect(page.charLength).toBe(4);
    expect(page.nextCharOffset).toBe(7);
    expect(page.hasMore).toBe(true);
    expect(createPaginationInfo(page)).toEqual({
      currentPage: 1,
      totalPages: 3,
      hasMore: true,
      charOffset: 3,
      charLength: 4,
      totalChars: 10,
    });
  });

  it('paginates by UTF-8 bytes and keeps returned string boundaries valid', () => {
    const content = 'aébc';
    const page = applyPagination(content, 1, 2, { mode: 'bytes' });

    expect(page.paginatedContent).toBe('é');
    expect(page.byteOffset).toBe(1);
    expect(page.byteLength).toBe(2);
    expect(page.nextByteOffset).toBe(3);
    expect(page.charOffset).toBe(1);
    expect(page.nextCharOffset).toBe(2);
  });

  it('slices from the containing line and extends through the next newline', () => {
    const text = 'first\n  second\nthird\nfourth';
    // sliceByCharRespectLines replaced by Rust sliceContent with snapToLineBoundary
    const slice = sliceContent(text, 8, 5, { snapToLineBoundary: true });

    // char 8 is inside "  second" (line starts at 6), snap start → 6
    expect(slice.charOffset).toBe(6);
    // snap to line boundary: sliced text starts at line start
    expect(slice.text.startsWith('  second')).toBe(true);
    expect(slice.hasMore).toBe(true);
    expect(slice.nextCharOffset).toBeDefined();
  });

  it('serializes compact and pretty pagination payloads deterministically', () => {
    const value = { b: 2, a: ['x'] };

    expect(serializeForPagination(value)).toBe('{"b":2,"a":["x"]}');
    expect(serializeForPagination(value, true)).toBe(
      '{\n  "b": 2,\n  "a": [\n    "x"\n  ]\n}'
    );
  });
});
