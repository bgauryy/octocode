import { describe, it, expect, vi } from 'vitest';

// Force the TypeScript parser to throw so tsJsStrategy falls back to the
// regex extractor (extractTsJsRegex) — the production safety net that is
// otherwise unreachable because ts.createSourceFile never throws on valid
// or invalid source text in normal operation.
vi.mock('typescript', async importOriginal => {
  const actual = await importOriginal<typeof import('typescript')>();
  const real = (actual as { default?: object }).default ?? actual;
  return {
    default: {
      ...real,
      createSourceFile: (): never => {
        throw new Error('forced parser failure');
      },
    },
  };
});

import { extractSignatures } from '@octocodeai/octocode-minifier';

const FALLBACK_SOURCE = `import { A } from './a';
import {
  C,
  D,
} from './b';

export interface Flat { id: string }

export enum Mode {
  On,
  Off,
}

export type Result<
  T,
> = Promise<
  T
>;

export type Simple = string;

export class Service {
  private name: string;

  public run(input: string): void {
    const hiddenBody = 1;
  }
}

export const handler = factory((ctx) => {
  const secretOne = 2;
});

export const fn = make(() =>
  inner(
    arg
  )
);

export function generic<
  A,
  B,
>(a: A, b: B): void {
  const hiddenGeneric = 3;
}

const plain = 1;
`;

describe('extractSignatures — regex fallback (TS parser unavailable)', () => {
  const sigs = (): string => extractSignatures(FALLBACK_SOURCE, 'fb.ts')!;

  it('still produces a skeleton when the TS parser throws', () => {
    expect(sigs()).not.toBeNull();
    expect(sigs().split('\n').length).toBeLessThan(
      FALLBACK_SOURCE.split('\n').length
    );
  });

  it('keeps single-line and multi-line imports with their names', () => {
    expect(sigs()).toContain("import { A } from './a';");
    expect(sigs()).toContain('C,');
    expect(sigs()).toContain('D,');
    expect(sigs()).toContain("} from './b';");
  });

  it('keeps single-line interface declarations whole', () => {
    expect(sigs()).toContain('export interface Flat { id: string }');
  });

  it('keeps enum bodies via brace-block tracking', () => {
    expect(sigs()).toContain('export enum Mode {');
    expect(sigs()).toContain('On,');
    expect(sigs()).toContain('Off,');
  });

  it('keeps multi-line type aliases until the terminating semicolon', () => {
    expect(sigs()).toContain('export type Result<');
    expect(sigs()).toContain('> = Promise<');
    expect(sigs()).toContain('>;');
    expect(sigs()).toContain('export type Simple = string;');
  });

  it('keeps class heads and modifier-prefixed members, drops bodies', () => {
    expect(sigs()).toContain('export class Service {');
    expect(sigs()).toContain('private name: string;');
    expect(sigs()).toContain('public run(input: string): void {');
    expect(sigs()).not.toContain('hiddenBody');
  });

  it('drops callback bodies of call-expression initializers, keeps closer', () => {
    expect(sigs()).toContain('export const handler = factory((ctx) => {');
    expect(sigs()).toContain('});');
    expect(sigs()).not.toContain('secretOne');
  });

  it('keeps multi-line call-initializer argument heads (paren tracking)', () => {
    expect(sigs()).toContain('export const fn = make(() =>');
    expect(sigs()).toContain('inner(');
    expect(sigs()).toContain('arg');
  });

  it('keeps multi-line generic signatures via angle-depth tracking', () => {
    expect(sigs()).toContain('export function generic<');
    expect(sigs()).toContain('A,');
    expect(sigs()).toContain('>(a: A, b: B): void {');
    expect(sigs()).not.toContain('hiddenGeneric');
  });

  it('drops plain non-exported assignments', () => {
    expect(sigs()).not.toContain('plain');
  });

  it('renders the gutter with original line numbers and no blank lines', () => {
    for (const line of sigs().split('\n')) {
      expect(line).toMatch(/^\s*\d+\|\s+\S/);
    }
  });
});
