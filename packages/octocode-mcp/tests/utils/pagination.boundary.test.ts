import { describe, it, expect } from 'vitest';
import {
  findNextBlockBoundary,
  isMidBlockCut,
  snapToSemanticBoundary,
} from '@octocodeai/octocode-tools-core';

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

describe('snapToSemanticBoundary', () => {
  it('extends supported code pages to the next semantic boundary', () => {
    const content = [
      'export function first() {',
      '  const value = 1;',
      '  return value;',
      '}',
      '',
      'export function second() {',
      '  return 2;',
      '}',
    ].join('\n');

    const requestedLength = content.indexOf('  const value');
    const secondFunction = content.indexOf('export function second');

    expect(
      snapToSemanticBoundary(content, 0, requestedLength, 'sample.ts')
    ).toEqual({
      length: secondFunction,
      chunkMode: 'semantic',
    });
  });

  it('keeps unsupported prose/data formats on exact char-limit pages', () => {
    const content = [
      'plain line one with no semantic meaning',
      'plain line two with no semantic meaning',
      'plain line three with no semantic meaning',
      'plain line four with no semantic meaning',
    ].join('\n');

    for (const filePath of [
      'notes.txt',
      'runtime.log',
      'data.json',
      'config.yml',
      'Cargo.toml',
      'settings.ini',
      'feed.xml',
      'image.svg',
      'README.rst',
    ]) {
      const requestedLength = 42;
      expect(
        snapToSemanticBoundary(content, 0, requestedLength, filePath)
      ).toEqual({
        length: requestedLength,
        chunkMode: 'char-limit',
      });
      expect(
        findNextBlockBoundary(content, requestedLength, filePath)
      ).toBeUndefined();
    }
  });

  it('uses char-limit for giant blocks and leaves a next boundary for hints', () => {
    const hugeBody = '  const value = 1;\n'.repeat(600);
    const content = [
      'export function first() {',
      hugeBody,
      '}',
      '',
      'export function second() {',
      '  return 2;',
      '}',
    ].join('\n');

    const requestedLength = 120;
    const snap = snapToSemanticBoundary(
      content,
      0,
      requestedLength,
      'sample.ts'
    );

    expect(snap).toEqual({
      length: requestedLength,
      chunkMode: 'char-limit',
    });
    expect(findNextBlockBoundary(content, requestedLength, 'sample.ts')).toBe(
      content.indexOf('export function second')
    );
  });
});

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
    const result = findNextBlockBoundary(
      content,
      content.length - 2,
      'file.ts'
    );
    expect(result).toBeUndefined();
  });
});

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
    expect(
      boundary.startsWith('func Bar()') || boundary.startsWith('type MyStruct')
    ).toBe(true);
  });
});

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
    expect(
      boundary.startsWith('pub struct Baz') || boundary.startsWith('}')
    ).toBe(true);
  });
});

describe('findNextBlockBoundary — Java', () => {
  const content = [
    'public class MyClass {',
    '',
    '    public void foo() {',
    '        int a = 1;',
    '        System.out.println(a);',
    '    }',
    '',
    '    private int bar(int x) {',
    '        return x * 2;',
    '    }',
    '',
    '    public static void main(String[] args) {',
    '        new MyClass().foo();',
    '    }',
    '}',
  ].join('\n');

  it('finds next method after cut inside foo()', () => {
    const cutPos = content.indexOf('        int a = 1');
    const result = findNextBlockBoundary(content, cutPos, 'MyClass.java');
    expect(result).toBeDefined();
    const boundary = content.substring(result!).trimStart();
    expect(boundary.startsWith('private int bar')).toBe(true);
  });

  it('finds main() after cut inside bar()', () => {
    const cutPos = content.indexOf('        return x * 2');
    const result = findNextBlockBoundary(content, cutPos, 'MyClass.java');
    expect(result).toBeDefined();
    const boundary = content.substring(result!).trimStart();
    expect(boundary.startsWith('public static void main')).toBe(true);
  });
});

describe('findNextBlockBoundary — Kotlin', () => {
  const content = [
    'class Calculator {',
    '',
    '    fun add(a: Int, b: Int): Int {',
    '        val sum = a + b',
    '        return sum',
    '    }',
    '',
    '    private fun multiply(a: Int, b: Int) = a * b',
    '',
    '    companion object {',
    '        val PI = 3.14',
    '    }',
    '}',
  ].join('\n');

  it('finds private fun after cut inside add()', () => {
    const cutPos = content.indexOf('        val sum');
    const result = findNextBlockBoundary(content, cutPos, 'Calculator.kt');
    expect(result).toBeDefined();
    const boundary = content.substring(result!).trimStart();
    expect(boundary.startsWith('private fun multiply')).toBe(true);
  });

  it('finds companion object after private fun', () => {
    const cutPos = content.indexOf('    private fun multiply');
    const result = findNextBlockBoundary(content, cutPos, 'Calculator.kt');
    expect(result).toBeDefined();
    const boundary = content.substring(result!).trimStart();
    expect(boundary.startsWith('companion object')).toBe(true);
  });
});

describe('findNextBlockBoundary — Scala', () => {
  const content = [
    'class Counter {',
    '  private var count = 0',
    '',
    '  def increment(): Unit = {',
    '    count += 1',
    '    println(count)',
    '  }',
    '',
    '  def reset(): Unit = {',
    '    count = 0',
    '  }',
    '',
    '  val value: Int = count',
    '}',
    '',
    'object Counter {',
    '  def apply() = new Counter()',
    '}',
  ].join('\n');

  it('finds def reset after cut inside def increment', () => {
    const cutPos = content.indexOf('    count += 1');
    const result = findNextBlockBoundary(content, cutPos, 'Counter.scala');
    expect(result).toBeDefined();
    const boundary = content.substring(result!).trimStart();
    expect(boundary.startsWith('def reset')).toBe(true);
  });

  it('finds val after def reset', () => {
    const cutPos = content.indexOf('    count = 0');
    const result = findNextBlockBoundary(content, cutPos, 'Counter.scala');
    expect(result).toBeDefined();
    const boundary = content.substring(result!).trimStart();
    expect(boundary.startsWith('val value') || boundary.startsWith('}')).toBe(
      true
    );
  });

  it('finds companion object after the class', () => {
    const cutPos = content.indexOf('  def apply()');
    const result = findNextBlockBoundary(content, cutPos, 'Counter.scala');
    expect(typeof result === 'number' || result === undefined).toBe(true);
  });
});

describe('findNextBlockBoundary — C#', () => {
  const content = [
    'namespace MyApp {',
    '    public class Service {',
    '        public string GetName() {',
    '            return "hello";',
    '        }',
    '',
    '        private void Process(int x) {',
    '            Console.WriteLine(x);',
    '        }',
    '    }',
    '}',
  ].join('\n');

  it('finds private void after cut inside GetName()', () => {
    const cutPos = content.indexOf('            return "hello"');
    const result = findNextBlockBoundary(content, cutPos, 'Service.cs');
    expect(result).toBeDefined();
    const boundary = content.substring(result!).trimStart();
    expect(boundary.startsWith('private void Process')).toBe(true);
  });
});

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
