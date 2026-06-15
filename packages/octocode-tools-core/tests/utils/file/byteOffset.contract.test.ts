import { describe, expect, it } from 'vitest';

import {
  byteToCharIndex,
  charToByteIndex,
  sliceContent,
} from '../../../src/utils/file/byteOffset.js';

describe('byte offset helpers contract', () => {
  it('uses JavaScript UTF-16 code-unit indexes for emoji content', () => {
    const content = 'a🌍b';

    expect(charToByteIndex(content, 0)).toBe(0);
    expect(charToByteIndex(content, 1)).toBe(1);
    expect(charToByteIndex(content, 3)).toBe(5);
    expect(charToByteIndex(content, 4)).toBe(6);

    expect(byteToCharIndex(content, 0)).toBe(0);
    expect(byteToCharIndex(content, 1)).toBe(1);
    expect(byteToCharIndex(content, 5)).toBe(3);
    expect(byteToCharIndex(content, 6)).toBe(4);
  });

  it('reports slice offsets and lengths as JavaScript UTF-16 indexes', () => {
    const page = sliceContent('a🌍b', 0, 3);

    expect(page.text).toBe('a🌍');
    expect(page.charOffset).toBe(0);
    expect(page.charLength).toBe(3);
    expect(page.byteOffset).toBe(0);
    expect(page.byteLength).toBe(5);
    expect(page.hasMore).toBe(true);
    expect(page.nextCharOffset).toBe(3);
  });
});
