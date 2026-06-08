import { describe, it, expect } from 'vitest';
import { extractSignatures } from '../../../src/utils/minifier/applyMinification.js';

const TS_SOURCE = `import { A, B } from './a';
import {
  C,
  D,
} from './b';

export interface Foo {
  id: string;
  name?: number;
}

export type Bar = {
  x: number;
};

export async function doThing(
  a: string,
  b: number,
): Promise<void> {
  const secretLocal = 1;
  return doSomethingElse(secretLocal);
}

function helper(): boolean {
  return true;
}

export const arrow = (x: number): string => {
  return String(x);
};
`;

describe('extractSignatures', () => {
  it('returns null for unrecognised extensions', () => {
    expect(extractSignatures('hello world', 'notes.unknownext')).toBeNull();
  });

  it('returns a non-null skeleton shorter than the source for code files', () => {
    const sigs = extractSignatures(TS_SOURCE, 'sample.ts');
    expect(sigs).not.toBeNull();
    expect(sigs!.length).toBeLessThan(TS_SOURCE.length);
  });

  it('keeps single-line and multi-line import statements with their names', () => {
    const sigs = extractSignatures(TS_SOURCE, 'sample.ts')!;
    expect(sigs).toContain("import { A, B } from './a';");
    // multi-line import names must survive, not just the `import {` opener
    expect(sigs).toContain('C,');
    expect(sigs).toContain('D,');
    expect(sigs).toContain("} from './b';");
  });

  it('keeps multi-line interface bodies (field names + types)', () => {
    const sigs = extractSignatures(TS_SOURCE, 'sample.ts')!;
    expect(sigs).toContain('interface Foo');
    expect(sigs).toContain('id: string;');
    expect(sigs).toContain('name?: number;');
  });

  it('keeps multi-line type alias bodies', () => {
    const sigs = extractSignatures(TS_SOURCE, 'sample.ts')!;
    expect(sigs).toContain('type Bar');
    expect(sigs).toContain('x: number;');
  });

  it('keeps the full multi-line function signature (params + return type)', () => {
    const sigs = extractSignatures(TS_SOURCE, 'sample.ts')!;
    expect(sigs).toContain('doThing(');
    expect(sigs).toContain('a: string,');
    expect(sigs).toContain('b: number,');
    expect(sigs).toContain('Promise<void>');
  });

  it('drops function bodies', () => {
    const sigs = extractSignatures(TS_SOURCE, 'sample.ts')!;
    expect(sigs).not.toContain('secretLocal');
    expect(sigs).not.toContain('doSomethingElse');
    expect(sigs).not.toContain('return String(x);');
  });

  it('keeps single-line function and arrow signatures', () => {
    const sigs = extractSignatures(TS_SOURCE, 'sample.ts')!;
    expect(sigs).toContain('function helper(): boolean');
    expect(sigs).toContain('export const arrow =');
  });

  it('returns null when no signatures are present (plain assignments only)', () => {
    expect(
      extractSignatures('const x = 1;\nconst y = 2;\n', 'plain.ts')
    ).toBeNull();
  });
});
