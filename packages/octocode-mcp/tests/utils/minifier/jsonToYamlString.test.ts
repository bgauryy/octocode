import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  jsonToYamlString,
  YamlConversionConfig,
} from '../../../src/utils/minifier/jsonToYamlString.js';

describe('jsonToYamlString', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  describe('Basic conversion', () => {
    it('should convert simple object to YAML', () => {
      const input = { name: 'John', age: 30, active: true };
      const yaml = jsonToYamlString(input);

      expect(yaml).toContain('name: "John"');
      expect(yaml).toContain('age: 30');
      expect(yaml).toContain('active: true');
    });

    it('should convert nested objects to YAML', () => {
      const input = {
        user: {
          name: 'Alice',
          settings: { theme: 'dark' },
        },
      };
      const yaml = jsonToYamlString(input);

      expect(yaml).toContain('user:');
      expect(yaml).toContain('  name: "Alice"');
      expect(yaml).toContain('  settings:');
      expect(yaml).toContain('    theme: "dark"');
    });

    it('should convert arrays to YAML', () => {
      const input = { tags: ['a', 'b', 'c'] };
      const yaml = jsonToYamlString(input);

      expect(yaml).toContain('tags:');
      expect(yaml).toContain('  - "a"');
      expect(yaml).toContain('  - "b"');
      expect(yaml).toContain('  - "c"');
    });

    it('should handle arrays of objects', () => {
      const input = {
        users: [
          { id: 1, name: 'User1' },
          { id: 2, name: 'User2' },
        ],
      };
      const yaml = jsonToYamlString(input);

      expect(yaml).toContain('users:');
      expect(yaml).toContain('  - id: 1');
      expect(yaml).toContain('    name: "User1"');
      expect(yaml).toContain('  - id: 2');
      expect(yaml).toContain('    name: "User2"');
    });
  });

  describe('Default behavior (no configuration)', () => {
    it('should preserve original key order when no config is provided', () => {
      const input = { name: 'Test', age: 30, id: 'abc' };
      const yaml = jsonToYamlString(input);
      const lines = yaml.split('\n').filter(line => line.trim());

      expect(lines[0]).toBe('name: "Test"');
      expect(lines[1]).toBe('age: 30');
      expect(lines[2]).toBe('id: "abc"');
    });

    it('should preserve original key order with empty config', () => {
      const input = { name: 'Test', age: 30, id: 'abc' };
      const yaml = jsonToYamlString(input, {});
      const lines = yaml.split('\n').filter(line => line.trim());

      expect(lines[0]).toBe('name: "Test"');
      expect(lines[1]).toBe('age: 30');
      expect(lines[2]).toBe('id: "abc"');
    });
  });

  describe('sortKeys configuration', () => {
    it('should sort keys alphabetically when sortKeys is true', () => {
      const input = { zebra: 1, apple: 2, mango: 3 };
      const config: YamlConversionConfig = { sortKeys: true };
      const yaml = jsonToYamlString(input, config);
      const lines = yaml.split('\n').filter(line => line.trim());

      expect(lines[0]).toBe('apple: 2');
      expect(lines[1]).toBe('mango: 3');
      expect(lines[2]).toBe('zebra: 1');
    });

    it('should sort nested object keys alphabetically', () => {
      const input = {
        outer: { zebra: 1, apple: 2 },
      };
      const config: YamlConversionConfig = { sortKeys: true };
      const yaml = jsonToYamlString(input, config);

      expect(yaml).toContain('outer:');
      expect(yaml).toContain('  apple: 2');
      expect(yaml).toContain('  zebra: 1');
      const appleIndex = yaml.indexOf('apple');
      const zebraIndex = yaml.indexOf('zebra');
      expect(appleIndex).toBeLessThan(zebraIndex);
    });

    it('should not sort when sortKeys is false', () => {
      const input = { zebra: 1, apple: 2, mango: 3 };
      const config: YamlConversionConfig = { sortKeys: false };
      const yaml = jsonToYamlString(input, config);
      const lines = yaml.split('\n').filter(line => line.trim());

      expect(lines[0]).toBe('zebra: 1');
      expect(lines[1]).toBe('apple: 2');
      expect(lines[2]).toBe('mango: 3');
    });
  });

  describe('keysPriority configuration', () => {
    it('should prioritize specified keys in order', () => {
      const input = { name: 'Test', id: 'abc', version: '1.0', type: 'lib' };
      const config: YamlConversionConfig = {
        keysPriority: ['id', 'type', 'name'],
      };
      const yaml = jsonToYamlString(input, config);
      const lines = yaml
        .split('\n')
        .filter(line => line.trim() && !line.startsWith(' '));

      expect(lines[0]).toBe('id: "abc"');
      expect(lines[1]).toBe('type: "lib"');
      expect(lines[2]).toBe('name: "Test"');
      expect(lines[3]).toBe('version: "1.0"');
    });

    it('should handle empty keysPriority array', () => {
      const input = { name: 'Test', id: 'abc' };
      const config: YamlConversionConfig = { keysPriority: [] };
      const yaml = jsonToYamlString(input, config);
      const lines = yaml.split('\n').filter(line => line.trim());

      expect(lines[0]).toBe('name: "Test"');
      expect(lines[1]).toBe('id: "abc"');
    });

    it('should handle keysPriority with non-existent keys', () => {
      const input = { name: 'Test', id: 'abc' };
      const config: YamlConversionConfig = {
        keysPriority: ['nonexistent', 'id', 'alsonotexist'],
      };
      const yaml = jsonToYamlString(input, config);
      const lines = yaml
        .split('\n')
        .filter(line => line.trim() && !line.startsWith(' '));

      expect(lines[0]).toBe('id: "abc"');
      expect(lines[1]).toBe('name: "Test"');
    });

    it('should work with nested objects', () => {
      const input = {
        user: { name: 'Alice', id: 'u1', age: 25 },
      };
      const config: YamlConversionConfig = {
        keysPriority: ['id', 'name'],
      };
      const yaml = jsonToYamlString(input, config);

      expect(yaml).toContain('user:');
      const lines = yaml.split('\n');
      const idLine = lines.findIndex(l => l.includes('id: "u1"'));
      const nameLine = lines.findIndex(l => l.includes('name: "Alice"'));
      const ageLine = lines.findIndex(l => l.includes('age: 25'));

      expect(idLine).toBeLessThan(nameLine);
      expect(nameLine).toBeLessThan(ageLine);
    });

    it('should handle multiple priority keys where both exist', () => {
      const input = { c: 3, a: 1, b: 2 };
      const config: YamlConversionConfig = {
        keysPriority: ['b', 'a'],
      };
      const yaml = jsonToYamlString(input, config);
      const lines = yaml.split('\n').filter(line => line.trim());

      expect(lines[0]).toBe('b: 2');
      expect(lines[1]).toBe('a: 1');
      expect(lines[2]).toBe('c: 3');
    });

    it('should handle case where only second key has priority', () => {
      const input = { x: 1, y: 2 };
      const config: YamlConversionConfig = {
        keysPriority: ['nonexistent', 'y'],
      };
      const yaml = jsonToYamlString(input, config);
      const lines = yaml.split('\n').filter(line => line.trim());

      expect(lines[0]).toBe('y: 2');
      expect(lines[1]).toBe('x: 1');
    });
  });

  describe('Combined sortKeys and keysPriority', () => {
    it('should place priority keys first, then sort remaining alphabetically', () => {
      const input = { zebra: 1, apple: 2, id: 'x', mango: 3 };
      const config: YamlConversionConfig = {
        sortKeys: true,
        keysPriority: ['id'],
      };
      const yaml = jsonToYamlString(input, config);
      const lines = yaml.split('\n').filter(line => line.trim());

      expect(lines[0]).toBe('id: "x"');
      expect(lines[1]).toBe('apple: 2');
      expect(lines[2]).toBe('mango: 3');
      expect(lines[3]).toBe('zebra: 1');
    });

    it('should place priority keys first, preserve original order when sortKeys is false', () => {
      const input = { zebra: 1, apple: 2, id: 'x', mango: 3 };
      const config: YamlConversionConfig = {
        sortKeys: false,
        keysPriority: ['id'],
      };
      const yaml = jsonToYamlString(input, config);
      const lines = yaml.split('\n').filter(line => line.trim());

      expect(lines[0]).toBe('id: "x"');
      expect(lines[1]).toBe('zebra: 1');
      expect(lines[2]).toBe('apple: 2');
      expect(lines[3]).toBe('mango: 3');
    });

    it('should handle multiple priority keys with sort', () => {
      const input = { d: 4, c: 3, b: 2, a: 1 };
      const config: YamlConversionConfig = {
        sortKeys: true,
        keysPriority: ['c', 'a'],
      };
      const yaml = jsonToYamlString(input, config);
      const lines = yaml.split('\n').filter(line => line.trim());

      expect(lines[0]).toBe('c: 3');
      expect(lines[1]).toBe('a: 1');
      expect(lines[2]).toBe('b: 2');
      expect(lines[3]).toBe('d: 4');
    });
  });

  describe('Edge cases for sorting', () => {
    it('should handle sortKeys with undefined config values', () => {
      const input = { b: 2, a: 1 };
      const config: YamlConversionConfig = {
        sortKeys: undefined,
        keysPriority: undefined,
      };
      const yaml = jsonToYamlString(input, config);
      const lines = yaml.split('\n').filter(line => line.trim());

      expect(lines[0]).toBe('b: 2');
      expect(lines[1]).toBe('a: 1');
    });

    it('should handle sortKeys true with empty keysPriority', () => {
      const input = { c: 3, a: 1, b: 2 };
      const config: YamlConversionConfig = {
        sortKeys: true,
        keysPriority: [],
      };
      const yaml = jsonToYamlString(input, config);
      const lines = yaml.split('\n').filter(line => line.trim());

      expect(lines[0]).toBe('a: 1');
      expect(lines[1]).toBe('b: 2');
      expect(lines[2]).toBe('c: 3');
    });
  });

  describe('Error handling', () => {
    it('should handle null values', () => {
      const input = { key: null };
      const yaml = jsonToYamlString(input);
      expect(yaml).toContain('key: null');
    });

    it('should handle undefined values by omitting them', () => {
      const input = { key: undefined, other: 'value' };
      const yaml = jsonToYamlString(input);
      expect(yaml).not.toContain('key:');
      expect(yaml).toContain('other: "value"');
    });

    it('should handle empty objects', () => {
      const yaml = jsonToYamlString({});
      expect(yaml.trim()).toBe('{}');
    });

    it('should handle empty arrays', () => {
      const yaml = jsonToYamlString([]);
      expect(yaml.trim()).toBe('[]');
    });

    it('should handle deeply nested structures', () => {
      const input = {
        level1: {
          level2: {
            level3: {
              value: 'deep',
            },
          },
        },
      };
      const yaml = jsonToYamlString(input);
      expect(yaml).toContain('level1:');
      expect(yaml).toContain('  level2:');
      expect(yaml).toContain('    level3:');
      expect(yaml).toContain('      value: "deep"');
    });
  });

  describe('Special values', () => {
    it('should handle numbers correctly', () => {
      const input = { int: 42, float: 3.14, negative: -10 };
      const yaml = jsonToYamlString(input);

      expect(yaml).toContain('int: 42');
      expect(yaml).toContain('float: 3.14');
      expect(yaml).toContain('negative: -10');
    });

    it('should handle boolean values', () => {
      const input = { yes: true, no: false };
      const yaml = jsonToYamlString(input);

      expect(yaml).toContain('yes: true');
      expect(yaml).toContain('no: false');
    });

    it('should quote all string values', () => {
      const input = { simple: 'hello', withSpace: 'hello world' };
      const yaml = jsonToYamlString(input);

      expect(yaml).toContain('simple: "hello"');
      expect(yaml).toContain('withSpace: "hello world"');
    });

    it('should handle special characters in strings', () => {
      const input = { special: 'line1\nline2', tabs: 'a\tb' };
      const yaml = jsonToYamlString(input);

      expect(yaml).toContain('special:');
      expect(yaml).toContain('tabs:');
    });

    it('should handle Date objects', () => {
      const date = new Date('2024-01-15T10:30:00Z');
      const input = { created: date };
      const yaml = jsonToYamlString(input);

      expect(yaml).toContain('created:');
    });

    it('should handle mixed arrays', () => {
      const input = { mixed: [1, 'two', true, null] };
      const yaml = jsonToYamlString(input);

      expect(yaml).toContain('mixed:');
      expect(yaml).toContain('  - 1');
      expect(yaml).toContain('  - "two"');
      expect(yaml).toContain('  - true');
      expect(yaml).toContain('  - null');
    });
  });
});
