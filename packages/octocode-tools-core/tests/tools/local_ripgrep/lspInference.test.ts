import { describe, expect, it } from 'vitest';

import {
  inferLspSymbolName,
  type LocalSearchEngine,
} from '../../../src/tools/local_ripgrep/ripgrepResultBuilder.js';
import type { RipgrepQuery } from '../../../src/tools/local_ripgrep/scheme.js';

// inferLspSymbolName only reads a few fields off the query; the rest of the
// RipgrepQuery surface is irrelevant to the inference decision.
const q = (fields: Partial<RipgrepQuery>): RipgrepQuery =>
  fields as unknown as RipgrepQuery;

const infer = (
  match:
    | {
        value?: string;
        metavars?: Record<string, string[]>;
        metavarRanges?: Record<string, Array<{ line: number }>>;
      }
    | undefined,
  query: Partial<RipgrepQuery>,
  engine: LocalSearchEngine = 'rg'
) => inferLspSymbolName(match, q(query), engine)?.symbol;

describe('inferLspSymbolName — known bad examples (must NOT infer)', () => {
  it('regex query \\w+_searched does not infer "w"', () => {
    expect(
      infer({ value: 'const w_searched = 1;' }, { keywords: '\\w+_searched' })
    ).toBeUndefined();
  });

  it('structural boolean capture does not infer "false"', () => {
    expect(
      infer(
        { value: 'setEnabled(false)', metavars: { V: ['false'] } },
        { mode: 'structural', pattern: 'setEnabled($V)' },
        'structural'
      )
    ).toBeUndefined();
  });

  it('text search for query.symbolName does not infer "query"', () => {
    expect(
      infer(
        { value: 'const result = query.symbolName;' },
        { keywords: 'query.symbolName' }
      )
    ).toBeUndefined();
  });

  it('multi-token snippet query does not infer', () => {
    expect(
      infer({ value: 'const foo = bar;' }, { keywords: 'const foo' })
    ).toBeUndefined();
  });

  it('dotted fixedString query does not infer', () => {
    expect(
      infer({ value: 'a.b.c()' }, { keywords: 'a.b.c', fixedString: true })
    ).toBeUndefined();
  });

  it('windowed onlyMatching match does not infer', () => {
    expect(
      infer(
        { value: '= getUser(' },
        { keywords: 'getUser', onlyMatching: true, matchWindow: 2 }
      )
    ).toBeUndefined();
  });

  it('aggregate count output does not infer', () => {
    expect(
      infer(
        { value: 'getUser' },
        { keywords: 'getUser', countMatchesPerFile: true }
      )
    ).toBeUndefined();
    expect(
      infer(
        { value: 'getUser' },
        { keywords: 'getUser', countLinesPerFile: true }
      )
    ).toBeUndefined();
  });

  it('unique onlyMatching output does not infer', () => {
    expect(
      infer(
        { value: 'getUser' },
        { keywords: 'getUser', onlyMatching: true, unique: true }
      )
    ).toBeUndefined();
  });

  it('reserved literals never infer (true/null/this/super/...)', () => {
    for (const lit of [
      'true',
      'false',
      'null',
      'undefined',
      'NaN',
      'Infinity',
      'this',
      'super',
    ]) {
      expect(infer({ value: lit }, { keywords: lit })).toBeUndefined();
    }
  });
});

describe('inferLspSymbolName — preserved good examples (must infer)', () => {
  it('exact bare-identifier query infers the symbol', () => {
    expect(
      infer({ value: 'function getUser() {}' }, { keywords: 'getUser' })
    ).toBe('getUser');
  });

  it('exact bare-identifier query infers even in fixedString mode', () => {
    expect(
      infer(
        { value: 'handleClick();' },
        { keywords: 'handleClick', fixedString: true }
      )
    ).toBe('handleClick');
  });

  it('exact onlyMatching value infers the symbol', () => {
    expect(
      infer(
        { value: 'createSession' },
        { keywords: 'create\\w+', onlyMatching: true }
      )
    ).toBe('createSession');
  });

  it('structural metavar bound to one bare identifier infers it', () => {
    expect(
      infer(
        { value: 'wrap(getUser)', metavars: { FN: ['getUser'] } },
        { mode: 'structural', pattern: 'wrap($FN)' },
        'structural'
      )
    ).toBe('getUser');
  });

  it('identifiers with $ and _ are valid bare identifiers', () => {
    expect(infer({ value: '_privateFn' }, { keywords: '_privateFn' })).toBe(
      '_privateFn'
    );
    expect(infer({ value: '$store' }, { keywords: '$store' })).toBe('$store');
  });
});

describe('inferLspSymbolName — precise metavarRanges line', () => {
  it('structural match with metavarRanges returns the capture line, not the match start', () => {
    const result = inferLspSymbolName(
      {
        value: 'class Wrapper {\n  getUser() {}\n}',
        metavars: { FN: ['getUser'] },
        metavarRanges: { FN: [{ line: 2 }] },
      },
      q({ mode: 'structural', pattern: 'class $C { $FN() {} }' }),
      'structural'
    );
    expect(result).toEqual({ symbol: 'getUser', line: 2 });
  });

  it('structural match without metavarRanges omits line (caller falls back to match start)', () => {
    const result = inferLspSymbolName(
      { value: 'wrap(getUser)', metavars: { FN: ['getUser'] } },
      q({ mode: 'structural', pattern: 'wrap($FN)' }),
      'structural'
    );
    expect(result).toEqual({ symbol: 'getUser', line: undefined });
  });

  it('non-structural inference never sets a precise line', () => {
    const result = inferLspSymbolName(
      { value: 'function getUser() {}' },
      q({ keywords: 'getUser' }),
      'rg'
    );
    expect(result).toEqual({ symbol: 'getUser', line: undefined });
  });
});
