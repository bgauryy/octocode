import { describe, expect, it } from 'vitest';

import { parseEnv } from '../src/dotenv.js';

describe('parseEnv', () => {
  it('parses KEY=VALUE pairs', () => {
    const m = parseEnv('A=1\nB=two\n');
    expect(m.A).toBe('1');
    expect(m.B).toBe('two');
  });

  it('strips surrounding double quotes', () => {
    expect(parseEnv('K="hello world"').K).toBe('hello world');
  });

  it('strips surrounding single quotes', () => {
    expect(parseEnv("K='v a l'").K).toBe('v a l');
  });

  it('skips a line with a blank key', () => {
    expect(parseEnv('=value\nKEY=v')).toEqual({ KEY: 'v' });
  });

  it('handles export prefix', () => {
    expect(parseEnv('export KEY=val').KEY).toBe('val');
  });

  it('ignores # comment lines', () => {
    const m = parseEnv('# comment\nA=1');
    expect('comment' in m).toBe(false);
    expect(m.A).toBe('1');
  });

  it('ignores lines without = sign', () => {
    const m = parseEnv('noequals\nA=1');
    expect('noequals' in m).toBe(false);
  });

  it('preserves = signs inside the value', () => {
    // Only the first = splits key from value
    expect(parseEnv('URL=https://example.com?a=1&b=2').URL).toBe(
      'https://example.com?a=1&b=2'
    );
  });

  it('handles CRLF line endings', () => {
    const m = parseEnv('A=1\r\nB=2\r\n');
    expect(m.A).toBe('1');
    expect(m.B).toBe('2');
  });

  it('allows empty value (KEY=)', () => {
    expect(parseEnv('EMPTY=').EMPTY).toBe('');
  });

  it('returns {} for null / undefined / empty string', () => {
    expect(parseEnv(null)).toEqual({});
    expect(parseEnv(undefined)).toEqual({});
    expect(parseEnv('')).toEqual({});
  });
});
