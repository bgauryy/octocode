import { describe, it, expect } from 'vitest';
import {
  extractSignatures,
  minifyCodeCore,
  minifyContentSync,
  minifyJsonCore,
  jsonToYamlString,
} from '@octocodeai/octocode-minifier';

describe('extractSignatures — AST edge cases', () => {
  it('keeps enum heads, members, and closer', () => {
    const src = `export enum Color {
  Red,
  Green,
}
`;
    const sigs = extractSignatures(src, 'colors.ts')!;
    expect(sigs).toContain('export enum Color {');
    expect(sigs).toContain('Red,');
    expect(sigs).toContain('Green,');
  });

  it('keeps declare-module blocks with their inner statements', () => {
    const src = `declare module 'pkg' {
  export function f(): void;
}
`;
    const sigs = extractSignatures(src, 'globals.d.ts')!;
    expect(sigs).toContain("declare module 'pkg' {");
    expect(sigs).toContain('export function f(): void;');
  });

  it('keeps bodyless ambient module declarations whole', () => {
    const sigs = extractSignatures("declare module 'bare';\n", 'amb.d.ts')!;
    expect(sigs).toContain("declare module 'bare';");
  });

  it('keeps class index signatures', () => {
    const src = `export class Dict {
  [key: string]: number;
}
`;
    const sigs = extractSignatures(src, 'dict.ts')!;
    expect(sigs).toContain('[key: string]: number;');
  });

  it('unwraps top-level await initializers', () => {
    const sigs = extractSignatures(
      'export const cfg = await loadConfig();\n',
      'cfg.ts'
    )!;
    expect(sigs).toContain('export const cfg = await loadConfig();');
  });

  it('unwraps parenthesized as-expressions in exported initializers', () => {
    const sigs = extractSignatures(
      'export const z = (config as Config);\n',
      'z.ts'
    )!;
    expect(sigs).toContain('export const z = (config as Config);');
  });

  it('drops multi-line object-literal interiors inside call initializers', () => {
    const src = `export const reg = register({
  secretKey: 1,
});
`;
    const sigs = extractSignatures(src, 'reg.ts')!;
    expect(sigs).toContain('export const reg = register({');
    expect(sigs).toContain('});');
    expect(sigs).not.toContain('secretKey');
  });

  it('keeps single-line expression-bodied arrows whole', () => {
    const sigs = extractSignatures(
      'export const inc = (x: number) => x + 1;\n',
      'inc.ts'
    )!;
    expect(sigs).toContain('export const inc = (x: number) => x + 1;');
  });

  it('keeps `exports.name = …` CommonJS surface, drops bodies', () => {
    const src = `exports.helper = function (a) {
  return a + 1;
};
`;
    const sigs = extractSignatures(src, 'mod.cjs')!;
    expect(sigs).toContain('exports.helper = function (a) {');
    expect(sigs).not.toContain('return a + 1;');
  });

  it('returns null instead of throwing on non-string content', () => {
    expect(extractSignatures(42 as unknown as string, 'x.ts')).toBeNull();
  });

  it('keeps the first gutter line aligned with multi-digit line numbers', () => {
    const lines = Array.from(
      { length: 12 },
      (_, i) => `export const v${i} = ${i};`
    );
    const sigs = extractSignatures(lines.join('\n'), 'wide.ts')!;
    const [first] = sigs.split('\n');
    // 12 kept lines → width 2 → line 1 must be padded to " 1| ".
    expect(first).toBe(' 1| export const v0 = 0;');
  });
});

describe('extractSignatures — heuristic strategy edge cases', () => {
  it('go: keeps multi-line func signature heads, drops bodies', () => {
    const src = `package main

func Process(
	a int,
	b string,
) error {
	return nil
}
`;
    const sigs = extractSignatures(src, 'main.go')!;
    expect(sigs).toContain('func Process(');
    expect(sigs).toContain('a int,');
    expect(sigs).toContain(') error {');
    expect(sigs).not.toContain('return nil');
  });

  it('go: filters c-style comment lines kept inside struct blocks', () => {
    const src = `package main

type Config struct {
	// note
	/* inline */
	/*
	 * block interior
	 */
	Name string
}
`;
    const sigs = extractSignatures(src, 'config.go')!;
    expect(sigs).toContain('Name string');
    expect(sigs).not.toContain('note');
    expect(sigs).not.toContain('inline');
    expect(sigs).not.toContain('block interior');
  });

  it('c: keeps multi-line parameter lists, drops function bodies', () => {
    const src = `#include <stdio.h>

static int add_numbers(
    int a,
    int b
) {
    return a + b;
}
`;
    const sigs = extractSignatures(src, 'add.c')!;
    expect(sigs).toContain('static int add_numbers(');
    expect(sigs).toContain('int a,');
    expect(sigs).not.toContain('return a + b;');
  });

  it('python: filters hash-comment lines kept inside multi-line def heads', () => {
    const src = `def f(
    # comment inside params
    a,
):
    pass
`;
    const sigs = extractSignatures(src, 'f.py')!;
    expect(sigs).toContain('def f(');
    expect(sigs).toContain('a,');
    expect(sigs).not.toContain('comment inside params');
  });

  it('html: skips multi-line comments and filters comment-wrapped keepables', () => {
    const src = `<!doctype html>
<!--
  <h1>commented heading</h1>
-->
<h1>Real Heading</h1>
<!-- <h2>inline commented</h2> -->
`;
    const sigs = extractSignatures(src, 'page.html')!;
    expect(sigs).toContain('Real Heading');
    expect(sigs).not.toContain('commented heading');
    expect(sigs).not.toContain('inline commented');
  });

  it('html: returns null when every keepable line is a comment', () => {
    expect(
      extractSignatures('<!-- <h1>hidden</h1> -->\n', 'empty.html')
    ).toBeNull();
  });

  it('vue: keeps external script src, skips comments and style bodies', () => {
    const src = `<!--
  file header
-->
<script src="./ext.js"></script>
<template>
  <div id="app"></div>
</template>
<style>
.a { color: red; }
</style>
`;
    const sigs = extractSignatures(src, 'App.vue')!;
    expect(sigs).toContain('<script src="./ext.js">');
    expect(sigs).toContain('<template>');
    expect(sigs).toContain('id="app"');
    expect(sigs).not.toContain('file header');
    expect(sigs).not.toContain('color: red');
  });

  it('vue: returns null when only style content exists', () => {
    expect(
      extractSignatures('<style>\n.a { c: 1; }\n</style>\n', 'Style.vue')
    ).toBeNull();
  });

  it('sql: tracks nested BEGIN…END bodies and filters kept comment lines', () => {
    const src = `CREATE PROCEDURE outer_proc()
BEGIN
  BEGIN
    SELECT secret_inner;
  END;
END;

CREATE TABLE t2 (
  id INT,
  -- col comment
  /* block c */
  /* dangling
  name TEXT
);
`;
    const sigs = extractSignatures(src, 'schema.sql')!;
    expect(sigs).toContain('CREATE PROCEDURE outer_proc()');
    expect(sigs).toContain('CREATE TABLE t2 (');
    expect(sigs).toContain('name TEXT');
    expect(sigs).not.toContain('secret_inner');
    expect(sigs).not.toContain('col comment');
    expect(sigs).not.toContain('block c');
    expect(sigs).not.toContain('dangling');
  });

  it('sql: handles CREATE FUNCTION heads ending at end-of-file', () => {
    const sigs = extractSignatures('CREATE FUNCTION lonely()', 'fn.sql')!;
    expect(sigs).toContain('CREATE FUNCTION lonely()');
  });

  it('shell: skips blank lines between a function head and its brace', () => {
    const src = `#!/bin/sh
greet()

{
  echo hidden
}
export GREETING=hello
`;
    const sigs = extractSignatures(src, 'greet.sh')!;
    expect(sigs).toContain('#!/bin/sh');
    expect(sigs).toContain('greet()');
    expect(sigs).toContain('export GREETING=hello');
    expect(sigs).not.toContain('echo hidden');
  });
});

describe('minifyContentSync — strategy routing edge cases', () => {
  it('routes sql through the aggressive core (comments stripped, collapsed)', () => {
    const result = minifyContentSync('SELECT  1;  -- note\n', 'q.sql');
    expect(result).toContain('SELECT 1;');
    expect(result).not.toContain('note');
  });

  it('falls back to the general strategy for an empty file path', () => {
    const result = minifyContentSync('hello   \n\n\n\nworld', '');
    expect(result).toBe('hello\n\nworld');
  });
});

describe('minifyCodeCore — indentation preservation', () => {
  it('keeps first-line indentation (gutter alignment) while dropping blank lines', () => {
    const skeleton = ' 1| import x;\n\n\n10| export y;\n';
    expect(minifyCodeCore(skeleton)).toBe(' 1| import x;\n\n10| export y;');
  });

  it('removes leading blank lines without touching first-line indent', () => {
    expect(minifyCodeCore('\n\n  indented();\n')).toBe('  indented();');
  });
});

describe('minifyJsonCore — malformed input edge cases', () => {
  it('returns the trimmed original when a trailing comma sits at EOF', () => {
    const result = minifyJsonCore('[1, 2,');
    expect(result.failed).toBe(false);
    expect(result.content).toBe('[1, 2,');
  });
});

describe('jsonToYamlString — block scalar edge cases', () => {
  it('preserves empty lines inside multi-line block scalars', () => {
    const yaml = jsonToYamlString({ note: 'first\n\nthird' });
    expect(yaml).toContain('note: |-');
    expect(yaml).toMatch(/note: \|-\n {2}first\n\n {2}third/);
  });
});
