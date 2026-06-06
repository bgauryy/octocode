import { describe, it, expect } from 'vitest';
import {
  byteSlice,
  byteToCharIndex,
  charToByteIndex,
  getByteLength,
  convertByteMatchToChar,
} from '../../../src/utils/file/byteOffset.js';

describe('byteOffset', () => {
  describe('byteSlice', () => {
    it('should extract ASCII substring correctly', () => {
      const content = 'Hello World';
      expect(byteSlice(content, 0, 5)).toBe('Hello');
      expect(byteSlice(content, 6, 11)).toBe('World');
    });

    it('should extract emoji correctly (4 bytes)', () => {
      const content = 'Hello 🌍 World';
      expect(byteSlice(content, 6, 10)).toBe('🌍');
    });

    it('should extract multi-byte characters correctly', () => {
      const content = '日本語';
      expect(byteSlice(content, 0, 3)).toBe('日');
      expect(byteSlice(content, 3, 6)).toBe('本');
      expect(byteSlice(content, 6, 9)).toBe('語');
    });

    it('should handle mixed ASCII and multi-byte characters', () => {
      const content = 'a日b本c語d';
      expect(byteSlice(content, 0, 1)).toBe('a');
      expect(byteSlice(content, 1, 4)).toBe('日');
      expect(byteSlice(content, 4, 5)).toBe('b');
    });

    it('should handle empty slice', () => {
      const content = 'Hello';
      expect(byteSlice(content, 0, 0)).toBe('');
      expect(byteSlice(content, 3, 3)).toBe('');
    });

    it('should handle full content', () => {
      const content = 'Hello 🌍';
      expect(byteSlice(content, 0, 10)).toBe('Hello 🌍');
    });
  });

  describe('byteToCharIndex', () => {
    it('should return 0 for byte offset 0', () => {
      expect(byteToCharIndex('Hello', 0)).toBe(0);
      expect(byteToCharIndex('🌍Hello', 0)).toBe(0);
    });

    it('should convert ASCII byte offsets correctly', () => {
      const content = 'Hello World';
      expect(byteToCharIndex(content, 5)).toBe(5);
      expect(byteToCharIndex(content, 6)).toBe(6);
    });

    it('should handle emoji byte offsets', () => {
      const content = 'Hello 🌍 World';
      expect(byteToCharIndex(content, 6)).toBe(6);
      expect(byteToCharIndex(content, 10)).toBe(8);
      expect(byteToCharIndex(content, 11)).toBe(9);
    });

    it('should handle Japanese characters', () => {
      const content = '日本語';
      expect(byteToCharIndex(content, 3)).toBe(1);
      expect(byteToCharIndex(content, 6)).toBe(2);
      expect(byteToCharIndex(content, 9)).toBe(3);
    });

    it('should clamp to content length', () => {
      const content = 'Hi';
      expect(byteToCharIndex(content, 100)).toBe(2);
    });
  });

  describe('charToByteIndex', () => {
    it('should convert ASCII char indices correctly', () => {
      const content = 'Hello World';
      expect(charToByteIndex(content, 5)).toBe(5);
      expect(charToByteIndex(content, 6)).toBe(6);
    });

    it('should handle emoji char indices', () => {
      const content = 'Hello 🌍 World';
      expect(charToByteIndex(content, 6)).toBe(6);
      expect(charToByteIndex(content, 8)).toBe(10);
      expect(charToByteIndex(content, 9)).toBe(11);
    });

    it('should handle Japanese characters', () => {
      const content = '日本語';
      expect(charToByteIndex(content, 1)).toBe(3);
      expect(charToByteIndex(content, 2)).toBe(6);
      expect(charToByteIndex(content, 3)).toBe(9);
    });

    it('should return 0 for char index 0', () => {
      expect(charToByteIndex('Hello', 0)).toBe(0);
      expect(charToByteIndex('🌍Hello', 0)).toBe(0);
    });
  });

  describe('getByteLength', () => {
    it('should return correct length for ASCII', () => {
      expect(getByteLength('Hello')).toBe(5);
      expect(getByteLength('')).toBe(0);
    });

    it('should return correct length for emoji', () => {
      expect(getByteLength('🌍')).toBe(4);
      expect(getByteLength('Hello 🌍')).toBe(10);
    });

    it('should return correct length for CJK characters', () => {
      expect(getByteLength('日')).toBe(3);
      expect(getByteLength('日本語')).toBe(9);
    });

    it('should return correct length for 2-byte characters', () => {
      expect(getByteLength('é')).toBe(2);
      expect(getByteLength('café')).toBe(5);
    });
  });

  describe('convertByteMatchToChar', () => {
    it('should convert ASCII match correctly', () => {
      const content = 'Hello World';
      const result = convertByteMatchToChar(content, 6, 5);

      expect(result.charOffset).toBe(6);
      expect(result.charLength).toBe(5);
      expect(result.text).toBe('World');
    });

    it('should convert emoji match correctly', () => {
      const content = 'Hello 🌍 World';
      const result = convertByteMatchToChar(content, 6, 4);

      expect(result.charOffset).toBe(6);
      expect(result.charLength).toBe(2);
      expect(result.text).toBe('🌍');
    });

    it('should convert match after emoji correctly', () => {
      const content = 'Hello 🌍 World';
      const result = convertByteMatchToChar(content, 11, 5);

      expect(result.charOffset).toBe(9);
      expect(result.charLength).toBe(5);
      expect(result.text).toBe('World');
    });

    it('should handle Japanese text match', () => {
      const content = 'Hello日本語World';
      const result = convertByteMatchToChar(content, 5, 9);

      expect(result.charOffset).toBe(5);
      expect(result.charLength).toBe(3);
      expect(result.text).toBe('日本語');
    });

    it('should handle empty match', () => {
      const content = 'Hello';
      const result = convertByteMatchToChar(content, 0, 0);

      expect(result.charOffset).toBe(0);
      expect(result.charLength).toBe(0);
      expect(result.text).toBe('');
    });
  });

  describe('roundtrip conversions', () => {
    it('should roundtrip ASCII correctly', () => {
      const content = 'Hello World';
      for (let i = 0; i <= content.length; i++) {
        const byteIdx = charToByteIndex(content, i);
        const charIdx = byteToCharIndex(content, byteIdx);
        expect(charIdx).toBe(i);
      }
    });

    it('should roundtrip emoji content correctly', () => {
      const content = 'a🌍b';

      expect(charToByteIndex(content, 0)).toBe(0);
      expect(charToByteIndex(content, 1)).toBe(1);
      expect(charToByteIndex(content, 3)).toBe(5);

      expect(byteToCharIndex(content, 0)).toBe(0);
      expect(byteToCharIndex(content, 1)).toBe(1);
      expect(byteToCharIndex(content, 5)).toBe(3);
    });
  });
});
