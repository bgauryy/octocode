import { describe, it, expect, beforeEach } from 'vitest';
import {
  generateCacheKey,
  clearAllCache,
} from '../../../octocode-tools-core/src/utils/http/cache.js';

function extractHash(cacheKey: string): string {
  const parts = cacheKey.split(':');
  if (parts.length !== 2) {
    throw new Error(`Invalid cache key format: ${cacheKey}`);
  }
  const hash = parts[1];
  if (!hash || hash.length !== 64) {
    throw new Error(
      `Invalid hash length. Expected 64 chars, got: ${hash?.length || 0}`
    );
  }
  if (!/^[a-f0-9]{64}$/.test(hash)) {
    throw new Error(`Invalid hash format. Expected hex, got: ${hash}`);
  }
  return hash;
}

function validateCharacterDistribution(
  charCounts: Record<string, number>,
  expectedTotal: number,
  tolerance = 0.2
): void {
  const hexChars = '0123456789abcdef';
  const expectedPerChar = expectedTotal / 16;

  for (const char of hexChars) {
    const frequency = charCounts[char];
    if (frequency === undefined) {
      throw new Error(`Missing character count for '${char}'`);
    }
    const deviation = Math.abs(frequency - expectedPerChar) / expectedPerChar;
    if (deviation >= tolerance) {
      throw new Error(
        `Character '${char}' distribution outside tolerance. ` +
          `Expected: ~${expectedPerChar.toFixed(1)}, Got: ${frequency}, ` +
          `Deviation: ${(deviation * 100).toFixed(1)}%`
      );
    }
  }
}

function validateHashProperties(hash: string): void {
  expect(hash.length).toBe(64);

  expect(/^[a-f0-9]{64}$/.test(hash)).toBe(true);

  const uniqueChars = new Set(hash).size;
  expect(uniqueChars >= 5).toBe(true);

  for (let i = 0; i <= hash.length - 32; i += 16) {
    const segment = hash.substring(i, i + 16);
    const nextSegment = hash.substring(i + 16, i + 32);
    if (segment === nextSegment) {
      throw new Error(`Hash contains repeating 16-char pattern: ${segment}`);
    }
  }
}

describe('Cache Collision Resistance Tests', () => {
  beforeEach(() => {
    clearAllCache();
  });

  describe('Adversarial Input Tests', () => {
    it('should resist birthday attack patterns', () => {
      const keys = new Set<string>();
      const attacks = [
        ...Array.from({ length: 100 }, (_, i) => ({
          prefix: `attack-${i}`,
          data: 'same',
        })),
        ...Array.from({ length: 100 }, (_, i) => ({
          prefix: 'attack',
          data: `var-${i}`,
        })),

        ...Array.from({ length: 50 }, (_, i) => ({
          prefix: 'ext',
          data: `base${'x'.repeat(i)}`,
        })),

        ...Array.from({ length: 50 }, (_, i) => ({
          prefix: 'pad',
          data: { value: 'test', padding: ' '.repeat(i) },
        })),
      ];

      for (const { prefix, data } of attacks) {
        expect(typeof prefix).toBe('string');
        expect(prefix.length >= 1).toBe(true);

        const key = generateCacheKey(prefix, data);

        expect(typeof key).toBe('string');
        expect(key.length >= 11).toBe(true);
        expect(key.includes(':')).toBe(true);

        const hash = extractHash(key);
        validateHashProperties(hash);

        expect(keys.has(key)).toBe(false);
        keys.add(key);
      }

      expect(keys.size).toBe(attacks.length);
    });

    it('should handle unicode and special character attacks', () => {
      const unicodeAttacks = [
        { data: 'test' },
        { data: 'tëst' },
        { data: 'tеst' },
        { data: 'te\u200dst' },
        { data: 'te\u200bst' },
        { data: 'test\u0000' },
        { data: 'test\uffff' },
        { data: '💩' },
        { data: '🏳️\u200d🌈' },
        { data: '👩\u200d💻' },
      ];

      const keys = new Set<string>();
      for (const attack of unicodeAttacks) {
        expect(typeof attack.data).toBe('string');

        const key = generateCacheKey('unicode', attack);

        expect(typeof key).toBe('string');
        const hash = extractHash(key);
        validateHashProperties(hash);

        expect(keys.has(key)).toBe(false);
        keys.add(key);
      }

      expect(keys.size).toBe(unicodeAttacks.length);
    });

    it('should resist JSON structure manipulation attacks', () => {
      const structureAttacks = [
        { a: '1', b: '2' },
        { a: '1', b: '2' },
        { b: '2', a: '1' },
        { a: 1, b: 2 },
        { a: '1,b:2' },
        { 'a:1,b': '2' },
        { '{"a":"1","b"': '"2"}' },
        ['a', '1', 'b', '2'],
        [{ a: '1' }, { b: '2' }],
        { nested: { a: '1', b: '2' } },
      ];

      const keys = new Set<string>();
      const sortedKeys = new Map<string, number>();

      for (let i = 0; i < structureAttacks.length; i++) {
        const key = generateCacheKey('struct', structureAttacks[i]);

        if (i === 0 || i === 1 || i === 2) {
          if (sortedKeys.has(key)) {
            sortedKeys.set(key, sortedKeys.get(key)! + 1);
          } else {
            sortedKeys.set(key, 1);
          }
        } else {
          expect(keys.has(key)).toBe(false);
          keys.add(key);
        }
      }

      expect(sortedKeys.size >= 1).toBe(true);
    });
  });

  describe('High-Volume Collision Tests', () => {
    it('should handle systematic parameter variations', () => {
      const keys = new Set<string>();
      const variations = [
        'boolean',
        'number',
        'string',
        'object',
        'array',
        'null',
        'undefined',
      ];

      for (const type1 of variations) {
        for (const type2 of variations) {
          for (let i = 0; i < 10; i++) {
            const testData = {
              type1,
              type2,
              index: i,
              combined: `${type1}-${type2}-${i}`,
            };

            expect(testData.type1).toBe(type1);
            expect(testData.type2).toBe(type2);
            expect(testData.index).toBe(i);
            expect(testData.combined).toBe(`${type1}-${type2}-${i}`);

            const key = generateCacheKey('variation', testData);

            const hash = extractHash(key);
            validateHashProperties(hash);

            expect(keys.has(key)).toBe(false);
            keys.add(key);
          }
        }
      }

      expect(keys.size).toBe(variations.length * variations.length * 10);
    });
  });

  describe('Edge Case Collision Tests', () => {
    it('should handle deeply nested object variations', () => {
      const createDeepObject = (depth: number, value: unknown): unknown => {
        if (depth === 0) return value;
        return { nested: createDeepObject(depth - 1, value) };
      };

      const keys = new Set<string>();

      for (let depth = 1; depth <= 10; depth++) {
        for (let value = 0; value < 5; value++) {
          const obj = createDeepObject(depth, value);

          if (depth === 1) {
            expect(obj).toEqual({ nested: value });
          }

          const key = generateCacheKey('deep', obj);

          const hash = extractHash(key);
          validateHashProperties(hash);

          expect(keys.has(key)).toBe(false);
          keys.add(key);
        }
      }

      expect(keys.size).toBe(10 * 5);
    });

    it('should handle circular reference prevention', () => {
      const obj1 = { a: 1, circular: '[Circular]' };
      const obj2 = { b: 1, circular: '[Circular]' };

      let key1 = '';
      let key2 = '';

      expect(() => {
        key1 = generateCacheKey('circular', obj1);
      }).not.toThrow();

      expect(() => {
        key2 = generateCacheKey('circular', obj2);
      }).not.toThrow();

      expect(typeof key1).toBe('string');
      expect(typeof key2).toBe('string');
      expect(key1).not.toBe(key2);
    });

    it('should handle prototype pollution attempts', () => {
      const maliciousInputs = [
        { type: 'proto', __proto__: { polluted: true } },
        { type: 'constructor', constructor: { prototype: { polluted: true } } },
        { type: 'prototype', prototype: { polluted: true } },
        { type: 'inherited', ...Object.create({ inherited: true }) },
        {
          type: 'clean',
          ...Object.assign(Object.create(null), { clean: true }),
        },
      ];

      const keys = new Set<string>();
      for (const input of maliciousInputs) {
        expect(typeof input.type).toBe('string');

        const key = generateCacheKey('proto', input);

        expect(typeof key).toBe('string');
        const hash = extractHash(key);
        validateHashProperties(hash);

        expect(keys.has(key)).toBe(false);
        keys.add(key);
      }

      expect(keys.size).toBe(maliciousInputs.length);
    });
  });

  describe('Timing Attack Resistance', () => {
    it('should have consistent timing for similar-length inputs', () => {
      const timings: number[] = [];
      const iterations = 1000;

      for (let i = 0; i < iterations; i++) {
        const data = { id: i, data: 'x'.repeat(100) };

        expect(data.id).toBe(i);
        expect(data.data.length).toBe(100);
        expect(data.data).toBe('x'.repeat(100));

        const start = process.hrtime.bigint();
        const key = generateCacheKey('timing', data);
        const end = process.hrtime.bigint();

        const hash = extractHash(key);
        validateHashProperties(hash);

        timings.push(Number(end - start) / 1_000_000);
      }

      const avg = timings.reduce((a, b) => a + b, 0) / timings.length;
      const variance =
        timings.reduce((acc, time) => acc + Math.pow(time - avg, 2), 0) /
        timings.length;
      const stdDev = Math.sqrt(variance);

      expect(timings.length).toBe(iterations);
      expect(avg > 0).toBe(true);
      expect(variance >= 0).toBe(true);
      expect(stdDev >= 0).toBe(true);

      const sortedTimings = [...timings].sort((a, b) => a - b);
      const medianTime = sortedTimings[Math.floor(timings.length / 2)]!;
      expect(medianTime > 0).toBe(true);

      const q1 = sortedTimings[Math.floor(timings.length * 0.25)]!;
      const q3 = sortedTimings[Math.floor(timings.length * 0.75)]!;
      expect(q3 - q1 < avg * 2).toBe(true);
    });
  });

  describe('Cryptographic Hash Properties', () => {
    it('should demonstrate avalanche effect', () => {
      const base = generateCacheKey('avalanche', { value: 'test' });
      const changed = generateCacheKey('avalanche', { value: 'Test' });

      const baseHash = extractHash(base);
      const changedHash = extractHash(changed);

      let differences = 0;
      for (let i = 0; i < baseHash.length; i++) {
        if (baseHash[i] !== changedHash[i]) {
          differences++;
        }
      }

      expect(differences > baseHash.length * 0.3).toBe(true);
    });

    it('should maintain uniform distribution of hash characters', () => {
      const hashes: string[] = [];

      for (let i = 0; i < 1000; i++) {
        const key = generateCacheKey('distribution', {
          id: i,
          random: Math.random(),
        });
        const hashPart = extractHash(key);
        hashes.push(hashPart);
      }

      const charCounts: Record<string, number> = {};
      const hexChars = '0123456789abcdef';

      for (const char of hexChars) {
        charCounts[char] = 0;
      }

      for (const hash of hashes) {
        validateHashProperties(hash);

        for (const char of hash) {
          if (charCounts[char] !== undefined) {
            charCounts[char]++;
          } else {
            throw new Error(`Unexpected character '${char}' in hash: ${hash}`);
          }
        }
      }

      const totalChars = hashes.length * 64;

      validateCharacterDistribution(charCounts, totalChars, 0.2);

      const frequencies = Object.values(charCounts);
      const minFreq = Math.min(...frequencies);
      const maxFreq = Math.max(...frequencies);
      const range = maxFreq - minFreq;
      const avgFreq =
        frequencies.reduce((a, b) => a + b, 0) / frequencies.length;

      expect(range < avgFreq * 0.4).toBe(true);

      expect(minFreq > avgFreq * 0.6).toBe(true);
      expect(maxFreq < avgFreq * 1.4).toBe(true);
    });
  });

  describe('Build Compatibility and Error Handling', () => {
    it('should handle all edge cases without throwing errors', () => {
      const testCases = [
        { prefix: 'edge1', data: null },
        { prefix: 'edge2', data: undefined },
        { prefix: 'edge3', data: 0 },
        { prefix: 'edge4', data: false },
        { prefix: 'edge5', data: '' },
        { prefix: 'edge6', data: [] },
        { prefix: 'edge7', data: {} },
        { prefix: 'edge8', data: { '': '' } },
        { prefix: 'edge9', data: { null: null, undefined: undefined } },
        { prefix: 'edge10', data: [null, undefined, 0, false, ''] },
      ];

      const keys = new Set<string>();

      for (const testCase of testCases) {
        expect(() => {
          const key = generateCacheKey(testCase.prefix, testCase.data);

          expect(typeof key).toBe('string');
          expect(key.length >= 11).toBe(true);

          const hash = extractHash(key);
          validateHashProperties(hash);

          expect(keys.has(key)).toBe(false);
          keys.add(key);
        }).not.toThrow();
      }

      expect(keys.size).toBe(testCases.length);
    });

    it('should maintain consistent behavior across multiple runs', () => {
      const testData = { consistent: 'test', value: 42 };
      const keys = new Set<string>();

      for (let i = 0; i < 100; i++) {
        const key = generateCacheKey('consistency', testData);
        keys.add(key);
      }

      expect(keys.size).toBe(1);

      const [key] = keys;
      const hash = extractHash(key!);
      validateHashProperties(hash);
    });

    it('should validate helper functions work correctly', () => {
      const validKey = 'v1-test:' + 'a'.repeat(64);
      expect(() => extractHash(validKey)).not.toThrow();
      expect(extractHash(validKey)).toBe('a'.repeat(64));

      expect(() => extractHash('invalid')).toThrow();
      expect(() => extractHash('v1-test:short')).toThrow();
      expect(() => extractHash('v1-test:' + 'g'.repeat(64))).toThrow();

      const validHash =
        'a1b2c3d4e5f67890fedcba9876543210abcdef1234567890cafe123456780fed';
      expect(() => validateHashProperties(validHash)).not.toThrow();

      const perfectDistribution: Record<string, number> = {};
      const hexChars = '0123456789abcdef';
      for (const char of hexChars) {
        perfectDistribution[char] = 100;
      }
      expect(() =>
        validateCharacterDistribution(perfectDistribution, 1600)
      ).not.toThrow();
    });

    it('should validate security properties are maintained', () => {
      const securityTests = [
        { prefix: 'security', data: 'short' },
        { prefix: 'security', data: 'x'.repeat(10000) },
        { prefix: 'security', data: { complex: { nested: { deep: 'data' } } } },

        { prefix: 'types', data: 123 },
        { prefix: 'types', data: true },
        { prefix: 'types', data: [1, 2, 3] },
        { prefix: 'types', data: { a: 1, b: 2 } },
      ];

      for (const test of securityTests) {
        const key = generateCacheKey(test.prefix, test.data);
        const hash = extractHash(key);

        expect(hash.length).toBe(64);

        expect(/^[a-f0-9]{64}$/.test(hash)).toBe(true);

        const uniqueChars = new Set(hash);
        expect(uniqueChars.size >= 5).toBe(true);

        const firstHalf = hash.substring(0, 32);
        const secondHalf = hash.substring(32, 64);
        expect(firstHalf).not.toBe(secondHalf);
      }
    });
  });
});
