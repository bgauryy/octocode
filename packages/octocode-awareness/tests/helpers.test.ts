import { describe, it, expect } from 'vitest';
import {
  utcNow, parseJsonList, tagsText, normalizeTags,
  normalizeReferences, normalizeLabel, normalizeFilePath, rowToMemory,
  MEMORY_LABELS, REFLECTION_IMPORTANCE,
} from '../src/helpers.js';
import { resolve } from 'node:path';

describe('utcNow', () => {
  it('returns ISO-8601 without milliseconds', () => {
    const ts = utcNow();
    expect(ts).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/);
  });

  it('two calls within 1s are close', () => {
    const a = new Date(utcNow()).getTime();
    const b = new Date(utcNow()).getTime();
    expect(b - a).toBeLessThan(1000);
  });
});

describe('parseJsonList', () => {
  it('returns [] for falsy', () => expect(parseJsonList(null)).toEqual([]));
  it('returns [] for empty string', () => expect(parseJsonList('')).toEqual([]));
  it('passes through an array', () => expect(parseJsonList(['a', 'b'])).toEqual(['a', 'b']));
  it('parses a JSON array string', () => expect(parseJsonList('["x","y"]')).toEqual(['x', 'y']));
  it('coerces non-string elements', () => expect(parseJsonList([1, 2])).toEqual(['1', '2']));
  it('returns [] on invalid JSON', () => expect(parseJsonList('{bad}')).toEqual([]));
  it('returns [] when JSON is not an array', () => expect(parseJsonList('{"a":1}')).toEqual([]));
  it('filters empty strings', () => expect(parseJsonList(['a', '', 'b'])).toEqual(['a', 'b']));
});

describe('tagsText', () => {
  it('empty array → single comma', () => expect(tagsText([])).toBe(','));
  it('wraps tags with commas', () => expect(tagsText(['a', 'b'])).toBe(',a,b,'));
  it('single tag', () => expect(tagsText(['x'])).toBe(',x,'));
});

describe('normalizeTags', () => {
  it('deduplicates', () => expect(normalizeTags(['a', 'a'])).toEqual(['a']));
  it('lowercases', () => expect(normalizeTags(['FOO'])).toEqual(['foo']));
  it('strips leading/trailing hyphens', () => expect(normalizeTags(['-foo-'])).toEqual(['foo']));
  it('replaces spaces with hyphens', () => expect(normalizeTags(['foo bar'])).toEqual(['foo-bar']));
  it('parses csv param', () => expect(normalizeTags([], 'a,b')).toEqual(['a', 'b']));
  it('merges tags and csv', () => expect(normalizeTags(['x'], 'y,x')).toEqual(['x', 'y']));
  it('filters empty after cleanup', () => expect(normalizeTags(['', '-', '  '])).toEqual([]));
});

describe('normalizeReferences', () => {
  it('deduplicates', () => expect(normalizeReferences(['a', 'a'])).toEqual(['a']));
  it('trims whitespace', () => expect(normalizeReferences(['  url  '])).toEqual(['url']));
  it('limits to 20', () => {
    const refs = Array.from({ length: 25 }, (_, i) => `ref${i}`);
    expect(normalizeReferences(refs)).toHaveLength(20);
  });
  it('truncates at 512 chars', () => {
    const long = 'x'.repeat(600);
    expect(normalizeReferences([long])[0]).toHaveLength(512);
  });
  it('filters empty strings', () => expect(normalizeReferences(['', 'ok', ''])).toEqual(['ok']));
});

describe('normalizeLabel', () => {
  it('uppercases valid labels', () => expect(normalizeLabel('gotcha')).toBe('GOTCHA'));
  it('defaults unknown to OTHER', () => expect(normalizeLabel('UNKNOWN')).toBe('OTHER'));
  it('defaults null/undefined to OTHER', () => {
    expect(normalizeLabel(null)).toBe('OTHER');
    expect(normalizeLabel(undefined)).toBe('OTHER');
  });
  it('handles spaces/dashes', () => expect(normalizeLabel('code review')).toBe('OTHER')); // no match
  it('all valid labels are recognised', () => {
    for (const label of MEMORY_LABELS) {
      expect(normalizeLabel(label)).toBe(label);
    }
  });
});

describe('normalizeFilePath', () => {
  it('returns null for falsy', () => {
    expect(normalizeFilePath(null)).toBeNull();
    expect(normalizeFilePath('')).toBeNull();
  });
  it('resolves to absolute path', () => {
    const result = normalizeFilePath('relative/path.ts');
    expect(result).toBe(resolve('relative/path.ts'));
  });
  it('keeps absolute paths intact', () => {
    expect(normalizeFilePath('/abs/path.ts')).toBe('/abs/path.ts');
  });
});

describe('REFLECTION_IMPORTANCE', () => {
  it('failed=8, partial=6, worked=5', () => {
    expect(REFLECTION_IMPORTANCE['failed']).toBe(8);
    expect(REFLECTION_IMPORTANCE['partial']).toBe(6);
    expect(REFLECTION_IMPORTANCE['worked']).toBe(5);
  });
});

describe('rowToMemory', () => {
  it('deserializes tags_json and leaves references empty when not joined', () => {
    const row = {
      memory_id: 'm1', agent_id: 'a', task_context: 't', observation: 'o',
      importance: 5, state: 'ACTIVE', label: 'BUG',
      superseded_by: null, tags_json: '["foo","bar"]',
      workspace_path: null, repo: null,
      ref: null, file_tree_fingerprint: null,
      novelty_score: null,
      last_accessed_at: null, access_count: 0, decay_half_life_days: null,
      failure_signature: null, valid_from: null, valid_to: null,
      expired_at: null, created_at: '2026-01-01T00:00:00Z', updated_at: null,
    };
    const mem = rowToMemory(row as Parameters<typeof rowToMemory>[0]);
    expect(mem.tags).toEqual(['foo', 'bar']);
    expect(mem.references).toEqual([]); // references live in memory_refs; empty when not joined
    expect(mem.state).toBe('ACTIVE');
  });

  it('returns empty references for a plain memory row', () => {
    const row = {
      memory_id: 'm2', agent_id: 'a', task_context: 't', observation: 'o',
      importance: 7, state: 'ACTIVE', label: 'DECISION',
      superseded_by: null, tags_json: '[]',
      workspace_path: null, repo: null, ref: null, file_tree_fingerprint: null,
      novelty_score: null, last_accessed_at: null, access_count: 0, decay_half_life_days: null,
      failure_signature: null, valid_from: null, valid_to: null,
      expired_at: null, created_at: '2026-01-01T00:00:00Z', updated_at: null,
    };
    const mem = rowToMemory(row as Parameters<typeof rowToMemory>[0]);
    // references are in memory_refs table; absent from plain m.* query -> empty array
    expect(mem.references).toEqual([]);
    expect(mem.tags).toEqual([]);
    expect(mem.importance).toBe(7);
  });
});
