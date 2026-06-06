import { beforeAll, describe, expect, it, vi } from 'vitest';

import {
  analyzeTreeSitterFile,
  getTreeSitterRuntime,
  resolveTreeSitter,
} from './tree-sitter.js';
import { DEFAULT_OPTS } from '../types/index.js';

import type { FlowMaps } from '../types/index.js';

const testOpts = { ...DEFAULT_OPTS, root: '/repo', emitTree: false };

let TREE_SITTER_AVAILABLE = false;

beforeAll(async () => {
  const runtime = await resolveTreeSitter();
  TREE_SITTER_AVAILABLE = runtime.available;
});

function emptyMaps(): FlowMaps {
  return { flowMap: new Map(), controlMap: new Map() };
}

describe('resolveTreeSitter', () => {
  it('returns a TreeSitterRuntime object', async () => {
    const runtime = await resolveTreeSitter();
    expect(runtime).toBeDefined();
    expect(typeof runtime.available).toBe('boolean');
    expect('parserTs' in runtime).toBe(true);
    expect('parserTsx' in runtime).toBe(true);
  });

  it('when available, sets parserTs and parserTsx', async () => {
    const runtime = await resolveTreeSitter();
    if (runtime.available) {
      expect(runtime.parserTs).not.toBeNull();
      expect(runtime.parserTsx).not.toBeNull();
    }
  });

  it('when not available, sets error and null parsers', async () => {
    const runtime = await resolveTreeSitter();
    if (!runtime.available) {
      expect(runtime.error).toBeDefined();
      expect(typeof runtime.error).toBe('string');
      expect(runtime.parserTs).toBeNull();
      expect(runtime.parserTsx).toBeNull();
    }
  });

  it('second call returns cached result (same object)', async () => {
    const first = await resolveTreeSitter();
    const second = await resolveTreeSitter();
    expect(first).toBe(second);
  });
});

describe('getTreeSitterRuntime', () => {
  it('returns null before resolveTreeSitter is called', async () => {
    vi.resetModules();
    const { getTreeSitterRuntime } = await import('./tree-sitter.js');
    expect(getTreeSitterRuntime()).toBeNull();
  });

  it('after resolveTreeSitter, returns the same runtime', async () => {
    const resolved = await resolveTreeSitter();
    const fromGet = getTreeSitterRuntime();
    expect(fromGet).not.toBeNull();
    expect(fromGet).toBe(resolved);
  });
});

describe('analyzeTreeSitterFile when runtime not available', () => {
  it('returns null when tree-sitter runtime has available: false', async () => {
    vi.doMock('tree-sitter', () => {
      throw new Error('tree-sitter not installed');
    });
    vi.doMock('tree-sitter-typescript', () => {
      throw new Error('tree-sitter-typescript not installed');
    });
    vi.resetModules();
    const { resolveTreeSitter: resolve, analyzeTreeSitterFile: analyze } =
      await import('./tree-sitter.js');
    const runtime = await resolve();
    expect(runtime.available).toBe(false);
    expect(runtime.error).toBeDefined();
    const result = analyze(
      '/repo/src/test.ts',
      'function foo() {}',
      testOpts,
      'test-pkg',
      null
    );
    expect(result).toBeNull();
  });
});

describe.skipIf(!TREE_SITTER_AVAILABLE)(
  'analyzeTreeSitterFile when runtime available',
  () => {
    it('parses a simple function file and extracts function with correct name, lineStart, complexity', () => {
      const code = `function greet() {
  return "hello";
}`;
      const result = analyzeTreeSitterFile(
        '/repo/src/greet.ts',
        code,
        testOpts,
        'test-pkg',
        null
      );
      expect(result).not.toBeNull();
      expect(result!.functions.length).toBe(1);
      expect(result!.functions[0].name).toBe('greet');
      expect(result!.functions[0].lineStart).toBe(1);
      expect(result!.functions[0].complexity).toBeGreaterThanOrEqual(1);
      expect(result!.parseEngine).toBe('tree-sitter');
      expect(result!.nodeCount).toBeGreaterThan(0);
    });

    it('parses file with multiple functions', () => {
      const code = `
function foo() { return 1; }
function bar() { return 2; }
const baz = () => 3;
`;
      const result = analyzeTreeSitterFile(
        '/repo/src/multi.ts',
        code,
        testOpts,
        'test-pkg',
        null
      );
      expect(result).not.toBeNull();
      expect(result!.functions.length).toBeGreaterThanOrEqual(2);
    });

    it('parses file with if/for/while control flows', () => {
      const code = `
function f(x: boolean) {
  if (x) { return 1; }
  for (let i = 0; i < 10; i++) { }
  while (false) { }
  return 0;
}
`;
      const result = analyzeTreeSitterFile(
        '/repo/src/flows.ts',
        code,
        testOpts,
        'test-pkg',
        null
      );
      expect(result).not.toBeNull();
      expect(result!.flows.length).toBeGreaterThan(0);
      const flowKinds = result!.flows.map(f => f.kind);
      expect(flowKinds).toContain('if_statement');
      expect(flowKinds).toContain('for_statement');
      expect(flowKinds).toContain('while_statement');
    });

    it('parses file with nested loops and tracks maxLoopDepth', () => {
      const code = `
function nested() {
  for (let i = 0; i < 10; i++) {
    for (let j = 0; j < 10; j++) {
      for (let k = 0; k < 10; k++) {}
    }
  }
}
`;
      const result = analyzeTreeSitterFile(
        '/repo/src/nested.ts',
        code,
        testOpts,
        'test-pkg',
        null
      );
      expect(result).not.toBeNull();
      const fn = result!.functions.find(f => f.name === 'nested');
      expect(fn).toBeDefined();
      expect(fn!.maxLoopDepth).toBe(3);
      expect(fn!.loops).toBe(3);
    });

    it('parses file with nested if and tracks maxBranchDepth', () => {
      const code = `
function branched(a: boolean, b: boolean) {
  if (a) {
    if (b) {
      return 1;
    }
  }
  return 0;
}
`;
      const result = analyzeTreeSitterFile(
        '/repo/src/branch.ts',
        code,
        testOpts,
        'test-pkg',
        null
      );
      expect(result).not.toBeNull();
      const fn = result!.functions.find(f => f.name === 'branched');
      expect(fn).toBeDefined();
      expect(fn!.maxBranchDepth).toBe(2);
    });

    it('parses file with async/await and counts awaits', () => {
      const code = `
async function fetchData() {
  const a = await fetch("a");
  const b = await fetch("b");
  return [a, b];
}
`;
      const result = analyzeTreeSitterFile(
        '/repo/src/async.ts',
        code,
        testOpts,
        'test-pkg',
        null
      );
      expect(result).not.toBeNull();
      const fn = result!.functions.find(f => f.name === 'fetchData');
      expect(fn).toBeDefined();
      expect(fn!.awaits).toBe(2);
    });

    it('arrow function in variable declaration gets correct name', () => {
      const code = `const handler = (x: number) => x + 1;`;
      const result = analyzeTreeSitterFile(
        '/repo/src/arrow.ts',
        code,
        testOpts,
        'test-pkg',
        null
      );
      expect(result).not.toBeNull();
      expect(result!.functions.length).toBe(1);
      expect(result!.functions[0].name).toBe('handler');
    });

    it('anonymous function gets <anonymous>', () => {
      const code = `(function() { return 42; })`;
      const result = analyzeTreeSitterFile(
        '/repo/src/anonymous.ts',
        code,
        testOpts,
        'test-pkg',
        null
      );
      expect(result).not.toBeNull();
      expect(result!.functions.length).toBe(1);
      expect(result!.functions[0].name).toBe('<anonymous>');
    });

    it('parses TSX file (parser selection)', () => {
      const code = `
function Component() {
  return <div>Hello</div>;
}
`;
      const result = analyzeTreeSitterFile(
        '/repo/src/Component.tsx',
        code,
        testOpts,
        'test-pkg',
        null
      );
      expect(result).not.toBeNull();
      expect(result!.functions.length).toBe(1);
      expect(result!.functions[0].name).toBe('Component');
    });

    it('extracts switch_statement as flow', () => {
      const code = `
function f(x: number) {
  switch (x) {
    case 1: return 1;
    case 2: return 2;
    default: return 0;
  }
}
`;
      const result = analyzeTreeSitterFile(
        '/repo/src/switch.ts',
        code,
        testOpts,
        'test-pkg',
        null
      );
      expect(result).not.toBeNull();
      const switchFlow = result!.flows.find(f => f.kind === 'switch_statement');
      expect(switchFlow).toBeDefined();
    });

    it('counts calls in function body', () => {
      const code = `
function f() {
  a();
  b();
  c();
}
`;
      const result = analyzeTreeSitterFile(
        '/repo/src/calls.ts',
        code,
        testOpts,
        'test-pkg',
        null
      );
      expect(result).not.toBeNull();
      const fn = result!.functions[0];
      expect(fn.calls).toBe(3);
    });

    it('increments complexity for ternary', () => {
      const code = `function f(x: boolean) { return x ? 1 : 0; }`;
      const result = analyzeTreeSitterFile(
        '/repo/src/ternary.ts',
        code,
        testOpts,
        'test-pkg',
        null
      );
      expect(result).not.toBeNull();
      const fn = result!.functions[0];
      expect(fn.complexity).toBeGreaterThan(1);
    });

    it('increments complexity for logical operators', () => {
      const code = `function f(a: boolean, b: boolean) { return a && b || !a; }`;
      const result = analyzeTreeSitterFile(
        '/repo/src/logical.ts',
        code,
        testOpts,
        'test-pkg',
        null
      );
      expect(result).not.toBeNull();
      const fn = result!.functions[0];
      expect(fn.complexity).toBeGreaterThan(1);
    });

    it('when emitTree is true, builds tree snapshot', () => {
      const code = `function foo() { return 1; }`;
      const opts = { ...testOpts, emitTree: true };
      const result = analyzeTreeSitterFile(
        '/repo/src/tree.ts',
        code,
        opts,
        'test-pkg',
        null
      );
      expect(result).not.toBeNull();
      expect(result!.tree).toBeDefined();
      expect(result!.tree!.kind).toBe('program');
      expect(result!.tree!.children.length).toBeGreaterThan(0);
    });

    it('populates maps when minFunctionStatements and minFlowStatements are met', () => {
      const code = `function bigFn() {
  const a = 1; const b = 2; const c = 3;
  const d = 4; const e = 5; const f = 6;
  return a + b + c + d + e + f;
}`;
      const maps = emptyMaps();
      const opts = {
        ...testOpts,
        thresholds: { ...testOpts.thresholds, minFunctionStatements: 6, minFlowStatements: 1 },
      };
      analyzeTreeSitterFile('/repo/src/big.ts', code, opts, 'test-pkg', maps);
      expect(maps.flowMap.size).toBeGreaterThan(0);
    });

    it('extracts function with correct params count', () => {
      const code = `function f(a: number, b: string, c: boolean) { return 1; }`;
      const result = analyzeTreeSitterFile(
        '/repo/src/params.ts',
        code,
        testOpts,
        'test-pkg',
        null
      );
      expect(result).not.toBeNull();
      const fn = result!.functions[0];
      expect(fn.params).toBeGreaterThanOrEqual(3);
    });

    it('sets source to tree-sitter on function entries', () => {
      const code = `function f() {}`;
      const result = analyzeTreeSitterFile(
        '/repo/src/source.ts',
        code,
        testOpts,
        'test-pkg',
        null
      );
      expect(result).not.toBeNull();
      expect(result!.functions[0].source).toBe('tree-sitter');
    });

    it('computes cognitiveComplexity > 0 for nested control flow', () => {
      const code = `function complexFn(x: number, y: boolean) {
  if (x > 0) {
    if (y) {
      for (let i = 0; i < x; i++) {
        if (i % 2 === 0) {
          console.log(i);
        }
      }
    }
  }
  return x;
}`;
      const result = analyzeTreeSitterFile(
        '/repo/src/cognitive.ts',
        code,
        testOpts,
        'test-pkg',
        null
      );
      expect(result).not.toBeNull();
      const fn = result!.functions[0];
      expect(fn.cognitiveComplexity).toBeGreaterThanOrEqual(10);
    });

    it('computes cognitiveComplexity = 0 for simple linear function', () => {
      const code = `function simple() {
  const a = 1;
  const b = 2;
  return a + b;
}`;
      const result = analyzeTreeSitterFile(
        '/repo/src/simple.ts',
        code,
        testOpts,
        'test-pkg',
        null
      );
      expect(result).not.toBeNull();
      expect(result!.functions[0].cognitiveComplexity).toBe(0);
    });

    it('handles else-if without double-counting nesting', () => {
      const code = `function classify(x: number) {
  if (x > 100) {
    return 'high';
  } else if (x > 50) {
    return 'medium';
  } else if (x > 0) {
    return 'low';
  } else {
    return 'none';
  }
}`;
      const result = analyzeTreeSitterFile(
        '/repo/src/elseif.ts',
        code,
        testOpts,
        'test-pkg',
        null
      );
      expect(result).not.toBeNull();
      const fn = result!.functions[0];
      expect(fn.cognitiveComplexity).toBeGreaterThan(0);
      expect(fn.cognitiveComplexity).toBeLessThan(10);
    });

    it('increments for logical operators (&&, ||, ??)', () => {
      const code = `function guard(a: any, b: any, c: any) {
  if (a && b || c) {
    return true;
  }
  return false;
}`;
      const result = analyzeTreeSitterFile(
        '/repo/src/logical.ts',
        code,
        testOpts,
        'test-pkg',
        null
      );
      expect(result).not.toBeNull();
      const fn = result!.functions[0];
      expect(fn.cognitiveComplexity).toBeGreaterThanOrEqual(3);
    });

    it('cognitiveComplexity reflects nesting depth penalty', () => {
      const shallow = `function shallow(x: boolean) { if (x) { return 1; } return 0; }`;
      const deep = `function deep(x: boolean, y: boolean) {
  if (x) {
    if (y) {
      return 1;
    }
  }
  return 0;
}`;
      const shallowResult = analyzeTreeSitterFile(
        '/repo/src/shallow.ts',
        shallow,
        testOpts,
        'test-pkg',
        null
      );
      const deepResult = analyzeTreeSitterFile(
        '/repo/src/deep.ts',
        deep,
        testOpts,
        'test-pkg',
        null
      );
      expect(shallowResult).not.toBeNull();
      expect(deepResult).not.toBeNull();
      expect(deepResult!.functions[0].cognitiveComplexity).toBeGreaterThan(
        shallowResult!.functions[0].cognitiveComplexity
      );
    });
  }
);
