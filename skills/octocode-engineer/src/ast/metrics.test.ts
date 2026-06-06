import * as ts from 'typescript';
import { describe, expect, it } from 'vitest';

import {
  collectMetrics,
  computeHalstead,
  computeMaintainabilityIndex,
  countLinesInNode,
} from './metrics.js';

function parse(code: string): ts.SourceFile {
  return ts.createSourceFile('test.ts', code, ts.ScriptTarget.ESNext, true);
}

function firstFn(sf: ts.SourceFile): ts.Node {
  let fn: ts.Node | undefined;
  ts.forEachChild(sf, n => {
    if (!fn && ts.isFunctionDeclaration(n)) fn = n;
  });
  return fn!;
}

function findNode(
  sf: ts.SourceFile,
  predicate: (n: ts.Node) => boolean
): ts.Node | undefined {
  let found: ts.Node | undefined;
  const visit = (n: ts.Node): void => {
    if (found) return;
    if (predicate(n)) {
      found = n;
      return;
    }
    ts.forEachChild(n, visit);
  };
  visit(sf);
  return found;
}

describe('collectMetrics', () => {
  it('empty function → complexity 1, all zeros except', () => {
    const sf = parse('function foo() {}');
    const fn = firstFn(sf);
    const m = collectMetrics(fn);
    expect(m.complexity).toBe(1);
    expect(m.maxBranchDepth).toBe(0);
    expect(m.maxLoopDepth).toBe(0);
    expect(m.returns).toBe(0);
    expect(m.awaits).toBe(0);
    expect(m.calls).toBe(0);
    expect(m.loops).toBe(0);
  });

  it('single if → complexity 2', () => {
    const sf = parse('function foo() { if (x) {} }');
    const fn = firstFn(sf);
    const m = collectMetrics(fn);
    expect(m.complexity).toBe(2);
  });

  it('nested if → depth 2', () => {
    const sf = parse('function foo() { if (a) { if (b) {} } }');
    const fn = firstFn(sf);
    const m = collectMetrics(fn);
    expect(m.maxBranchDepth).toBe(2);
  });

  it('for loop → loops 1, maxLoopDepth 1', () => {
    const sf = parse('function foo() { for (let i = 0; i < 10; i++) {} }');
    const fn = firstFn(sf);
    const m = collectMetrics(fn);
    expect(m.loops).toBe(1);
    expect(m.maxLoopDepth).toBe(1);
  });

  it('nested for loops → maxLoopDepth 2', () => {
    const sf = parse(
      'function foo() { for (let i = 0; i < 5; i++) { for (let j = 0; j < 5; j++) {} } }'
    );
    const fn = firstFn(sf);
    const m = collectMetrics(fn);
    expect(m.loops).toBe(2);
    expect(m.maxLoopDepth).toBe(2);
  });

  it('while loop → complexity+1', () => {
    const sf = parse('function foo() { while (x) {} }');
    const fn = firstFn(sf);
    const m = collectMetrics(fn);
    expect(m.complexity).toBe(2);
  });

  it('do-while loop → complexity+1', () => {
    const sf = parse('function foo() { do {} while (x); }');
    const fn = firstFn(sf);
    const m = collectMetrics(fn);
    expect(m.complexity).toBe(2);
  });

  it('switch statement → complexity+1', () => {
    const sf = parse('function foo() { switch (x) { case 1: break; } }');
    const fn = firstFn(sf);
    const m = collectMetrics(fn);
    expect(m.complexity).toBe(2);
  });

  it('catch clause → complexity+1', () => {
    const sf = parse('function foo() { try {} catch (e) {} }');
    const fn = firstFn(sf);
    const m = collectMetrics(fn);
    expect(m.complexity).toBe(2);
  });

  it('ternary (ConditionalExpression) → complexity+1', () => {
    const sf = parse('function foo() { return x ? 1 : 2; }');
    const fn = firstFn(sf);
    const m = collectMetrics(fn);
    expect(m.complexity).toBe(2);
  });

  it('return/throw → returns', () => {
    const sf = parse('function foo() { return 1; }');
    const fn = firstFn(sf);
    const m = collectMetrics(fn);
    expect(m.returns).toBe(1);
  });

  it('throw → returns', () => {
    const sf = parse('function foo() { throw new Error(); }');
    const fn = firstFn(sf);
    const m = collectMetrics(fn);
    expect(m.returns).toBe(1);
  });

  it('await expression → awaits', () => {
    const sf = parse('async function foo() { await bar(); }');
    const fn = firstFn(sf);
    const m = collectMetrics(fn);
    expect(m.awaits).toBe(1);
  });

  it('call expression → calls', () => {
    const sf = parse('function foo() { bar(); }');
    const fn = firstFn(sf);
    const m = collectMetrics(fn);
    expect(m.calls).toBe(1);
  });

  it('logical && → complexity+1', () => {
    const sf = parse('function foo() { if (a && b) {} }');
    const fn = firstFn(sf);
    const m = collectMetrics(fn);
    expect(m.complexity).toBe(3);
  });

  it('logical || → complexity+1', () => {
    const sf = parse('function foo() { if (a || b) {} }');
    const fn = firstFn(sf);
    const m = collectMetrics(fn);
    expect(m.complexity).toBe(3);
  });

  it('for-in loop → loops 1', () => {
    const sf = parse('function foo() { for (const k in obj) {} }');
    const fn = firstFn(sf);
    const m = collectMetrics(fn);
    expect(m.loops).toBe(1);
    expect(m.maxLoopDepth).toBe(1);
  });

  it('for-of loop → loops 1', () => {
    const sf = parse('function foo() { for (const x of arr) {} }');
    const fn = firstFn(sf);
    const m = collectMetrics(fn);
    expect(m.loops).toBe(1);
    expect(m.maxLoopDepth).toBe(1);
  });
});

describe('computeHalstead', () => {
  it('empty function → all zeros or near-zero', () => {
    const sf = parse('function foo() {}');
    const fn = firstFn(sf);
    const h = computeHalstead(fn);
    expect(h.operators).toBe(0);
    expect(h.distinctOperators).toBe(0);
    expect(h.vocabulary).toBeLessThanOrEqual(1);
    expect(h.length).toBeLessThanOrEqual(1);
    expect(h.volume).toBe(0);
    expect(h.difficulty).toBe(0);
    expect(h.effort).toBe(0);
    expect(h.time).toBe(0);
    expect(h.estimatedBugs).toBe(0);
  });

  it('simple expression a + b → distinct operators/operands', () => {
    const sf = parse('function foo() { const x = a + b; }');
    const fn = firstFn(sf);
    const h = computeHalstead(fn);
    expect(h.distinctOperators).toBeGreaterThan(0);
    expect(h.distinctOperands).toBeGreaterThan(0);
    expect(h.operators).toBeGreaterThan(0);
    expect(h.operands).toBeGreaterThan(0);
  });

  it('repeated identifiers → distinct vs total count', () => {
    const sf = parse('function foo() { const x = a + a + a; }');
    const fn = firstFn(sf);
    const h = computeHalstead(fn);
    expect(h.operands).toBeGreaterThan(h.distinctOperands);
  });

  it('string and numeric literals counted as operands', () => {
    const sf = parse('function foo() { const x = "hello" + 42; }');
    const fn = firstFn(sf);
    const h = computeHalstead(fn);
    expect(h.operands).toBeGreaterThan(0);
    expect(h.distinctOperands).toBeGreaterThan(0);
  });

  it('volume, difficulty, effort are > 0 for non-trivial code', () => {
    const sf = parse('function foo() { const x = a + b * c; return x; }');
    const fn = firstFn(sf);
    const h = computeHalstead(fn);
    expect(h.volume).toBeGreaterThan(0);
    expect(h.difficulty).toBeGreaterThan(0);
    expect(h.effort).toBeGreaterThan(0);
  });

  it('various operator tokens (=, ==, ===, +, -, *, /)', () => {
    const sf = parse(
      'function foo() { const x = a == b ? c : d; const y = a + b - c * d / e; }'
    );
    const fn = firstFn(sf);
    const h = computeHalstead(fn);
    expect(h.distinctOperators).toBeGreaterThan(2);
    expect(h.operators).toBeGreaterThan(2);
  });
});

describe('computeMaintainabilityIndex', () => {
  it('MI for simple code → high score (near 100)', () => {
    const mi = computeMaintainabilityIndex(1, 1, 1);
    expect(mi).toBeGreaterThan(95);
    expect(mi).toBeLessThanOrEqual(100);
  });

  it('MI for complex code → lower score', () => {
    const simple = computeMaintainabilityIndex(100, 5, 50);
    const complex = computeMaintainabilityIndex(5000, 30, 500);
    expect(complex).toBeLessThan(simple);
  });

  it('MI never below 0', () => {
    const mi = computeMaintainabilityIndex(100000, 100, 10000);
    expect(mi).toBeGreaterThanOrEqual(0);
  });

  it('increasing halsteadVolume decreases MI', () => {
    const low = computeMaintainabilityIndex(100, 5, 50);
    const high = computeMaintainabilityIndex(5000, 5, 50);
    expect(high).toBeLessThan(low);
  });

  it('increasing cyclomaticComplexity decreases MI', () => {
    const low = computeMaintainabilityIndex(100, 5, 50);
    const high = computeMaintainabilityIndex(100, 30, 50);
    expect(high).toBeLessThan(low);
  });

  it('increasing linesOfCode decreases MI', () => {
    const low = computeMaintainabilityIndex(100, 5, 50);
    const high = computeMaintainabilityIndex(100, 5, 500);
    expect(high).toBeLessThan(low);
  });
});

describe('countLinesInNode', () => {
  it('single line function → 1', () => {
    const sf = parse('function foo() { return 1; }');
    const fn = firstFn(sf);
    const lines = countLinesInNode(sf, fn);
    expect(lines).toBe(1);
  });

  it('multi-line function → correct count', () => {
    const code = `function foo() {
  const a = 1;
  const b = 2;
  return a + b;
}`;
    const sf = parse(code);
    const fn = firstFn(sf);
    const lines = countLinesInNode(sf, fn);
    expect(lines).toBe(5);
  });

  it('inline function → 1', () => {
    const sf = parse('const fn = () => 1;');
    const fn = findNode(sf, ts.isArrowFunction)!;
    const lines = countLinesInNode(sf, fn);
    expect(lines).toBe(1);
  });
});
