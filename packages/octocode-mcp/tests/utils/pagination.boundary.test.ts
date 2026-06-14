import { describe, it, expect } from 'vitest';
import {
  findNextBlockBoundary,
  isMidBlockCut,
} from '@octocodeai/octocode-tools-core';

// ── isMidBlockCut ─────────────────────────────────────────────────────────────

describe('isMidBlockCut', () => {
  it('returns true when last meaningful line is indented (mid-function)', () => {
    const page = 'function foo() {\n  const a = 1;\n  const b =';
    expect(isMidBlockCut(page)).toBe(true);
  });

  it('returns true for tab-indented content', () => {
    const page = 'function bar() {\n\tconst x = 1;';
    expect(isMidBlockCut(page)).toBe(true);
  });

  it('returns false when last meaningful line is at column 0', () => {
    const page = 'function foo() {\n  return 1;\n}\n\nexport const bar = 2;';
    expect(isMidBlockCut(page)).toBe(false);
  });

  it('returns false for content ending with a closing brace at col 0', () => {
    const page = 'function foo() {\n  return 1;\n}';
    expect(isMidBlockCut(page)).toBe(false);
  });

  it('returns false for empty content', () => {
    expect(isMidBlockCut('')).toBe(false);
  });
});

// ── findNextBlockBoundary — TypeScript / JavaScript ───────────────────────────

describe('findNextBlockBoundary — TS/JS', () => {
  const content = [
    'function foo() {',
    '  const a = 1;',
    '  return a;',
    '}',
    '',
    'export function bar() {',
    '  return 42;',
    '}',
    '',
    'const baz = 3;',
  ].join('\n');

  it('finds export function after a cut inside foo()', () => {
    // Cut inside foo body (after the opening line)
    const cutPos = content.indexOf('  const a');
    const result = findNextBlockBoundary(content, cutPos, 'file.ts');
    expect(result).toBeDefined();
    const boundary = content.substring(result!);
    expect(boundary.startsWith('export function bar()')).toBe(true);
  });

  it('finds const after a cut inside bar()', () => {
    const cutPos = content.indexOf('  return 42');
    const result = findNextBlockBoundary(content, cutPos, 'utils.js');
    expect(result).toBeDefined();
    const boundary = content.substring(result!);
    expect(boundary.startsWith('const baz')).toBe(true);
  });

  it('returns undefined when no boundary exists after cut', () => {
    // Cut at the very end — no more top-level definitions
    const result = findNextBlockBoundary(content, content.length - 2, 'file.ts');
    expect(result).toBeUndefined();
  });
});

// ── findNextBlockBoundary — Python ───────────────────────────────────────────

describe('findNextBlockBoundary — Python', () => {
  const content = [
    'def foo():',
    '    a = 1',
    '    return a',
    '',
    'def bar():',
    '    return 42',
    '',
    'class Baz:',
    '    pass',
  ].join('\n');

  it('finds def bar after a cut inside def foo', () => {
    const cutPos = content.indexOf('    a = 1');
    const result = findNextBlockBoundary(content, cutPos, 'module.py');
    expect(result).toBeDefined();
    const boundary = content.substring(result!);
    expect(boundary.startsWith('def bar()')).toBe(true);
  });

  it('finds class Baz after a cut inside def bar', () => {
    const cutPos = content.indexOf('    return 42');
    const result = findNextBlockBoundary(content, cutPos, 'module.py');
    expect(result).toBeDefined();
    const boundary = content.substring(result!);
    expect(boundary.startsWith('class Baz')).toBe(true);
  });
});

// ── findNextBlockBoundary — Go ───────────────────────────────────────────────

describe('findNextBlockBoundary — Go', () => {
  const content = [
    'func Foo() {',
    '\tx := 1',
    '\treturn',
    '}',
    '',
    'func Bar() {',
    '\treturn',
    '}',
    '',
    'type MyStruct struct {',
    '\tField int',
    '}',
  ].join('\n');

  it('finds func Bar after a cut inside Foo', () => {
    const cutPos = content.indexOf('\tx := 1');
    const result = findNextBlockBoundary(content, cutPos, 'main.go');
    expect(result).toBeDefined();
    const boundary = content.substring(result!);
    expect(boundary.startsWith('func Bar()')).toBe(true);
  });

  it('finds type after a cut inside Bar', () => {
    const cutPos = content.indexOf('\treturn');
    const result = findNextBlockBoundary(content, cutPos, 'main.go');
    expect(result).toBeDefined();
    const boundary = content.substring(result!);
    expect(boundary.startsWith('func Bar()') || boundary.startsWith('type MyStruct')).toBe(true);
  });
});

// ── findNextBlockBoundary — Rust ─────────────────────────────────────────────

describe('findNextBlockBoundary — Rust', () => {
  const content = [
    'pub fn foo() {',
    '    let x = 1;',
    '}',
    '',
    'impl MyStruct {',
    '    pub fn bar(&self) {',
    '        let y = 2;',
    '    }',
    '}',
    '',
    'pub struct Baz {',
    '    field: i32,',
    '}',
  ].join('\n');

  it('finds impl after cut inside foo', () => {
    const cutPos = content.indexOf('    let x = 1');
    const result = findNextBlockBoundary(content, cutPos, 'lib.rs');
    expect(result).toBeDefined();
    const boundary = content.substring(result!);
    expect(boundary.startsWith('impl MyStruct')).toBe(true);
  });

  it('finds pub struct after cut inside impl', () => {
    const cutPos = content.indexOf('        let y = 2');
    const result = findNextBlockBoundary(content, cutPos, 'lib.rs');
    expect(result).toBeDefined();
    const boundary = content.substring(result!);
    expect(boundary.startsWith('pub struct Baz') || boundary.startsWith('}')).toBe(true);
  });
});

// ── findNextBlockBoundary — generic fallback ─────────────────────────────────

describe('findNextBlockBoundary — generic (unknown extension)', () => {
  const content = [
    'something_at_top_level {',
    '  inner_content;',
    '}',
    '',
    'next_top_level {',
    '  more;',
    '}',
  ].join('\n');

  it('finds next top-level line for unknown extension', () => {
    const cutPos = content.indexOf('  inner_content');
    const result = findNextBlockBoundary(content, cutPos, 'file.xyz');
    expect(result).toBeDefined();
    const boundary = content.substring(result!);
    expect(boundary.startsWith('next_top_level')).toBe(true);
  });

  it('works without a file path', () => {
    const cutPos = content.indexOf('  inner_content');
    const result = findNextBlockBoundary(content, cutPos);
    expect(result).toBeDefined();
  });
});
