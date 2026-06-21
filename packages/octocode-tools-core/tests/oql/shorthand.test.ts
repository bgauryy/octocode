import { describe, expect, it } from 'vitest';
import { buildShorthandInput } from '../../src/oql/shorthand.js';
import { normalizeQuery } from '../../src/oql/normalize.js';

function input(r: ReturnType<typeof buildShorthandInput>) {
  if ('error' in r) throw new Error(r.error);
  return r.input;
}

describe('buildShorthandInput (CLI shorthand lowering, owned by tools-core)', () => {
  it('text + local corpus -> text sugar that normalizes to code/text', () => {
    const sugar = input(
      buildShorthandInput({
        text: 'runCLI',
        corpus: { kind: 'local', path: './src' },
      })
    );
    expect(sugar).toMatchObject({ text: 'runCLI', path: './src' });
    const n = normalizeQuery(sugar as never);
    expect(n.target).toBe('code');
    expect(n.where).toEqual({ kind: 'text', value: 'runCLI' });
  });

  it('pattern requires lang', () => {
    const r = buildShorthandInput({
      pattern: 'eval($X)',
      corpus: { kind: 'local', path: '.' },
    });
    expect('error' in r && r.error).toMatch(/--pattern requires --lang/);
  });

  it('regex + pcre2 -> canonical regex where with dialect', () => {
    const sugar = input(
      buildShorthandInput({
        regex: 'a(?=b)',
        pcre2: true,
        corpus: { kind: 'local', path: '.' },
      })
    );
    expect((sugar as { where?: unknown }).where).toMatchObject({
      kind: 'regex',
      value: 'a(?=b)',
      dialect: 'pcre2',
    });
  });

  it('github corpus + type -> repo/path/ref + langType', () => {
    const sugar = input(
      buildShorthandInput({
        text: 'useEffect',
        type: 'tsx',
        corpus: {
          kind: 'github',
          repo: 'facebook/react',
          path: 'packages/react',
          ref: 'main',
        },
      })
    );
    expect(sugar).toMatchObject({
      text: 'useEffect',
      repo: 'facebook/react',
      path: 'packages/react',
      ref: 'main',
      langType: 'tsx',
    });
  });

  it('predicate precedence pattern > regex > text', () => {
    const sugar = input(
      buildShorthandInput({
        pattern: 'foo($X)',
        lang: 'ts',
        regex: 'bar',
        text: 'baz',
        corpus: { kind: 'local', path: '.' },
      })
    );
    expect(sugar.pattern).toBe('foo($X)');
    expect((sugar as { regex?: unknown }).regex).toBeUndefined();
    expect((sugar as { text?: unknown }).text).toBeUndefined();
  });
});
