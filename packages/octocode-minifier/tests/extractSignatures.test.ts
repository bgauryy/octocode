import { describe, it, expect } from 'vitest';
import { extractSignatures } from '@octocodeai/octocode-minifier';

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

const GENERIC_SOURCE = `export type Params<
  T extends Base,
  K extends keyof T = never,
> = Flatten<Omit<T, K>>;

export function generic<
  A,
  B,
>(a: A, b: B): [A, B] {
  return [a, b];
}
`;

describe('extractSignatures', () => {
  it('returns null for unrecognised extensions', () => {
    expect(extractSignatures('hello world', 'notes.unknownext')).toBeNull();
  });

  it('keeps multi-line generic type-alias declarations whole (params + RHS)', () => {
    const sigs = extractSignatures(GENERIC_SOURCE, 'g.ts')!;
    expect(sigs).toContain('export type Params<');
    expect(sigs).toContain('T extends Base,');
    expect(sigs).toContain('K extends keyof T = never,');
    expect(sigs).toContain('> = Flatten<Omit<T, K>>;');
  });

  it('keeps multi-line generic function signatures (generics + params + return)', () => {
    const sigs = extractSignatures(GENERIC_SOURCE, 'g.ts')!;
    expect(sigs).toContain('export function generic<');
    expect(sigs).toContain('A,');
    expect(sigs).toContain('B,');
    expect(sigs).toContain('(a: A, b: B): [A, B]');
    expect(sigs).not.toContain('return [a, b];');
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

  it('drops callback bodies of single-line call-expression initializers', () => {
    const src = `export const ZodString: core.$constructor<ZodString> = /*@__PURE__*/ core.$constructor("ZodString", (inst, def) => {
  core.$ZodString.init(inst, def);
  inst.format = bag.format ?? null;
  inst.regex = (regex, params) => inst.check(core._regex(regex, params));
});
`;
    const sigs = extractSignatures(src, 'schemas.ts')!;
    expect(sigs).toContain('export const ZodString:');
    expect(sigs).toContain('});');
    expect(sigs).not.toContain('init(inst, def)');
    expect(sigs).not.toContain('inst.format');
    expect(sigs).not.toContain('inst.regex');
  });

  it('drops callback bodies of multi-line call-expression initializers (keeps head args)', () => {
    const src = `export const ZodTransform: core.$constructor<ZodTransform> = /*@__PURE__*/ core.$constructor(
  "ZodTransform",
  (inst, def) => {
    core.$ZodTransform.init(inst, def);
    if (_ctx.direction === "backward") {
      throw new core.$ZodEncodeError(inst.constructor.name);
    }
  }
);
`;
    const sigs = extractSignatures(src, 'schemas.ts')!;
    expect(sigs).toContain('export const ZodTransform:');
    expect(sigs).toContain('"ZodTransform",');
    expect(sigs).toContain('(inst, def) => {');
    expect(sigs).toContain(');');
    expect(sigs).not.toContain('init(inst, def)');
    expect(sigs).not.toContain('backward');
    expect(sigs).not.toContain('ZodEncodeError');
  });

  it('does not keep bare control-flow statements (lone `if (...)` lines)', () => {
    const src = `export function check(result: Result | null): Result {
  if (!result)
    return fallback();
  if (result.ok && result.value)
    return result;
  return fallback();
}
`;
    const sigs = extractSignatures(src, 'npm.ts')!;
    expect(sigs).toContain('export function check(');
    expect(sigs).not.toContain('if (');
  });

  it('drops the body of functions with multi-line object-typed params (keeps params)', () => {
    const src = `export function _function(params?: {
  output?: core.$ZodType;
  input?: core.$ZodFunctionArgs;
}): ZodFunction {
  return new ZodFunction({
    type: "function",
  });
}
`;
    const sigs = extractSignatures(src, 'schemas.ts')!;
    expect(sigs).toContain('export function _function(params?: {');
    expect(sigs).toContain('output?: core.$ZodType;');
    expect(sigs).toContain('}): ZodFunction {');
    expect(sigs).not.toContain('return new ZodFunction');
    expect(sigs).not.toContain('type: "function"');
  });

  it('emits no blank numbered gutter lines', () => {
    const src = `export interface Spaced {
  a: string;

  b: number;
}
`;
    const sigs = extractSignatures(src, 'spaced.ts')!;
    for (const line of sigs.split('\n')) {
      expect(line).toMatch(/^\s*\d+\|\s+\S/);
    }
  });
});
