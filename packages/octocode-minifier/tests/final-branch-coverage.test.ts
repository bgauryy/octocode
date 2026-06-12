// Final branch-coverage tests — targets every remaining uncovered line/branch
// identified from the v8 coverage report.
//
// Gaps closed here:
//  1. strategies.ts:1038  — minifyComponentAsync empty-content early return
//  2. strategies.ts:861   — minifyTypeScriptLikeSync 'Unknown error' branch
//                           (non-Error object thrown by minify_sync)
//  3. extractSignatures.ts:976  — Vue script[lang=ts] block (TypeScript path)
//  4. extractSignatures.ts:1032 — SQL multi-line block comment close
//  5. extractSignatures.ts:1141 — shell function, next non-blank NOT a brace
//  6. apply.ts:24         — getBaseName empty-string fallback
//  7. jsonToYamlString.ts:98-103 — circular/non-serialisable double-fallback
//
// Defensive lines that remain uncovered (true last-resort fallbacks):
//  - strategies.ts:1058  — minifyComponentAsync catch block
//  - minifier.ts:294-299 — minifyContent outer catch
//  - minifier.ts:283     — component result.reason spread

import { describe, it, expect, vi } from 'vitest';
import {
  minifyComponentAsync,
  minifyTypeScriptLikeSync,
  extractSignatures,
  applyContentViewMinification,
  jsonToYamlString,
} from '@octocodeai/octocode-minifier';

// ─── mock setup for strategies that rely on terser ────────────────────────────

const mockMinifySync = vi.hoisted(() => vi.fn());
vi.mock('terser', async importOriginal => {
  const actual = await importOriginal<typeof import('terser')>();
  return {
    ...actual,
    minify_sync: mockMinifySync,
  };
});

// ─────────────────────────────────────────────────────────────────────────────
// 1. minifyComponentAsync — empty / whitespace-only content (line 1038)
// ─────────────────────────────────────────────────────────────────────────────

describe('minifyComponentAsync — empty content early return', () => {
  it('returns original content unchanged for an empty string', async () => {
    const result = await minifyComponentAsync('');
    expect(result.failed).toBe(false);
    expect(result.content).toBe('');
    expect(result.reason).toBeUndefined();
  });

  it('returns original content unchanged for whitespace-only input', async () => {
    const result = await minifyComponentAsync('   \n\t  ');
    expect(result.failed).toBe(false);
    expect(result.content).toBe('   \n\t  ');
  });

  it('returns original content unchanged for newline-only input', async () => {
    const result = await minifyComponentAsync('\n\n\n');
    expect(result.failed).toBe(false);
    expect(result.content).toBe('\n\n\n');
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 2. minifyTypeScriptLikeSync — non-Error thrown by minify_sync (line 861)
// ─────────────────────────────────────────────────────────────────────────────

describe('minifyTypeScriptLikeSync — non-Error object thrown by minify_sync', () => {
  it('returns failed:true with "Unknown error" reason when minify_sync throws a string', () => {
    // Force minify_sync to throw a raw string (not an Error instance) so the
    // ternary (error instanceof Error ? error.message : 'Unknown error') takes
    // the right-hand branch.
    mockMinifySync.mockImplementationOnce(() => {
      throw 'raw string error';
    });

    const result = minifyTypeScriptLikeSync(
      'export const x: number = 1;',
      'x.ts'
    );

    expect(result.failed).toBe(true);
    expect(result.reason).toContain('Unknown error');
    // Content falls back to the transpiled JS (before terser step)
    expect(typeof result.content).toBe('string');
  });

  it('returns failed:true with "Unknown error" reason when minify_sync throws null', () => {
    mockMinifySync.mockImplementationOnce(() => {
      throw null;
    });

    const result = minifyTypeScriptLikeSync('const y = 2;', 'y.ts');

    expect(result.failed).toBe(true);
    expect(result.reason).toContain(
      'Terser minification failed after TypeScript transpilation'
    );
    expect(result.reason).toContain('Unknown error');
  });

  it('still includes the error message when minify_sync throws a real Error', () => {
    mockMinifySync.mockImplementationOnce(() => {
      throw new Error('parse failure');
    });

    const result = minifyTypeScriptLikeSync('const z = 3;', 'z.ts');

    expect(result.failed).toBe(true);
    expect(result.reason).toContain('parse failure');
    // This exercises the (error instanceof Error ? error.message) TRUE branch
    expect(result.reason).not.toContain('Unknown error');
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 3. extractSignatures — Vue <script lang="ts"> (line 976)
// ─────────────────────────────────────────────────────────────────────────────

const VUE_TS_SFC = `<template>
  <div>{{ message }}</div>
</template>
<script lang="ts">
import { defineComponent, ref } from 'vue';

interface Config {
  debug: boolean;
}

export default defineComponent({
  name: 'TsPanel',
  setup() {
    const message = ref('hello');
    const secret = 'should drop';
    return { message };
  },
});
</script>
<style>
.panel { color: red; }
</style>
`;

describe('extractSignatures — Vue <script lang="ts"> TypeScript path', () => {
  const sigs = extractSignatures(VUE_TS_SFC, 'TsPanel.vue')!;

  it('returns a non-null skeleton', () => {
    expect(sigs).not.toBeNull();
  });

  it('keeps the import inside the script block', () => {
    expect(sigs).toContain("import { defineComponent, ref } from 'vue'");
  });

  it('keeps the interface declaration', () => {
    expect(sigs).toContain('interface Config');
  });

  it('keeps the defineComponent export head', () => {
    expect(sigs).toContain('export default defineComponent(');
  });

  it('drops function bodies inside the script block', () => {
    expect(sigs).not.toContain("'should drop'");
    expect(sigs).not.toContain('secret');
  });

  it('keeps the <template> block head', () => {
    expect(sigs).toContain('<template>');
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 4. extractSignatures — SQL multi-line block comment close (line 1032)
// ─────────────────────────────────────────────────────────────────────────────

const SQL_WITH_MULTILINE_COMMENT = `/*
 * Schema for the orders subsystem.
 * Generated 2024-01-01.
 */
CREATE TABLE orders (
  id    INT PRIMARY KEY,
  total DECIMAL(10,2)
);

/*
 * Helper view — read-only.
 */
CREATE VIEW order_summary AS
  SELECT id, total FROM orders WHERE total > 0;

CREATE FUNCTION compute_tax(amount DECIMAL)
RETURNS DECIMAL
$$
DECLARE
  secret_rate DECIMAL := 0.1;
BEGIN
  RETURN amount * secret_rate;
END;
$$;
`;

describe('extractSignatures — SQL multi-line block comment (line 1032)', () => {
  const sigs = extractSignatures(SQL_WITH_MULTILINE_COMMENT, 'schema.sql')!;

  it('returns a non-null skeleton', () => {
    expect(sigs).not.toBeNull();
  });

  it('drops multi-line block comment bodies that close with */', () => {
    expect(sigs).not.toContain('Schema for the orders subsystem');
    expect(sigs).not.toContain('Helper view');
    expect(sigs).not.toContain('Generated 2024');
  });

  it('keeps CREATE TABLE head after comment', () => {
    expect(sigs).toContain('CREATE TABLE orders');
    expect(sigs).toContain('id    INT PRIMARY KEY');
    expect(sigs).toContain('total DECIMAL(10,2)');
  });

  it('keeps CREATE VIEW head after second comment', () => {
    expect(sigs).toContain('CREATE VIEW order_summary AS');
  });

  it('keeps CREATE FUNCTION head', () => {
    expect(sigs).toContain('CREATE FUNCTION compute_tax(amount DECIMAL)');
  });

  it('drops the $$-quoted function body', () => {
    expect(sigs).not.toContain('secret_rate');
    expect(sigs).not.toContain('RETURN amount * secret_rate');
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// Additional SQL — $$ dollar-quoting body tracking (line 1036 inDollar exit)
// ─────────────────────────────────────────────────────────────────────────────

describe('extractSignatures — SQL $$ dollar-quoting body', () => {
  const src = `CREATE FUNCTION add_nums(a INT, b INT)
RETURNS INT
$$
  SELECT a + b;
$$;

CREATE TABLE users (
  id   INT,
  name TEXT
);
`;

  const sigs = extractSignatures(src, 'funcs.sql')!;

  it('keeps the function head and signature', () => {
    expect(sigs).toContain('CREATE FUNCTION add_nums(a INT, b INT)');
  });

  it('drops the $$ body contents', () => {
    expect(sigs).not.toContain('SELECT a + b;');
  });

  it('keeps CREATE TABLE after the function', () => {
    expect(sigs).toContain('CREATE TABLE users');
    expect(sigs).toContain('id   INT');
    expect(sigs).toContain('name TEXT');
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 5. extractSignatures — shell function where next non-blank line is NOT '{'
//    (line 1141 FALSE branch — depth===0 but no brace follows)
// ─────────────────────────────────────────────────────────────────────────────

describe('extractSignatures — shell function without trailing brace', () => {
  it('handles a function declaration not followed by { on any next line', () => {
    // greet() matches SH_FUNC; shellBraceDelta('greet()') === 0 so we enter
    // the brace-search loop. After skipping the blank line, the next non-blank
    // line is 'export', which does NOT start with '{'. So the if at line 1141
    // evaluates to FALSE (depth stays 0) and we skip the while loop.
    const src = `#!/bin/sh
greet()

export GREETING=hello
`;
    const sigs = extractSignatures(src, 'greet.sh')!;

    expect(sigs).not.toBeNull();
    expect(sigs).toContain('#!/bin/sh');
    expect(sigs).toContain('greet()');
    expect(sigs).toContain('export GREETING=hello');
  });

  it('handles a function declaration where EOF follows directly (no next line)', () => {
    const src = `#!/bin/bash\nprocess()\n`;
    const sigs = extractSignatures(src, 'proc.bash');

    // May return null (too short) or a skeleton — just must not throw
    expect(() => extractSignatures(src, 'proc.bash')).not.toThrow();
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 6. apply.ts:24 — getBaseName empty fallback (|| '')
// ─────────────────────────────────────────────────────────────────────────────

describe('applyContentViewMinification — empty basename fallback', () => {
  it('handles a file path ending with a path separator (basename is empty)', () => {
    // 'some/path/'.split(/[\\/]/).pop() returns '' (empty string, falsy),
    // triggering the || '' fallback in getBaseName.
    const content = 'hello world\n\n\nline3\n';
    const result = applyContentViewMinification(content, 'some/path/');

    // Unknown extension → general strategy; should not throw
    expect(typeof result).toBe('string');
    expect(result.length).toBeLessThanOrEqual(content.length);
  });

  it('handles a bare slash path without throwing', () => {
    const content = 'a\nb\nc\n';
    const result = applyContentViewMinification(content, '/');

    expect(typeof result).toBe('string');
  });

  it('handles an empty file path without throwing', () => {
    const content = 'some text\n\n\nmore text\n';
    const result = applyContentViewMinification(content, '');

    expect(typeof result).toBe('string');
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 7. jsonToYamlString.ts:98-103 — double-fallback for non-serialisable objects
// ─────────────────────────────────────────────────────────────────────────────

describe('jsonToYamlString — double-fallback for circular / non-serialisable objects', () => {
  it('falls back to JSON.stringify when js-yaml fails (circular reference)', () => {
    // Circular references cause js-yaml with noRefs:true to throw. The outer
    // catch tries JSON.stringify next; JSON.stringify also throws on circular
    // references, triggering the innermost catch that returns the error comment.
    const circular: Record<string, unknown> = {};
    circular.self = circular;

    const result = jsonToYamlString(circular);

    // Must return a string (never throw)
    expect(typeof result).toBe('string');
    // Result is the error comment block (both serialisers failed)
    expect(result).toContain('# YAML conversion failed:');
  });

  it('result contains both failure messages when both serialisers fail', () => {
    const circular: Record<string, unknown> = { a: 1 };
    circular.loop = circular;

    const result = jsonToYamlString(circular);

    expect(result).toMatch(/# YAML conversion failed:/);
    expect(result).toMatch(/# JSON conversion also failed:/);
  });

  it('does not throw for deeply circular structures', () => {
    const a: Record<string, unknown> = {};
    const b: Record<string, unknown> = { ref: a };
    a.ref = b;

    expect(() => jsonToYamlString(a)).not.toThrow();
    expect(typeof jsonToYamlString(a)).toBe('string');
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 8. extractSignatures — CSS multi-line block comment close (line 839)
// ─────────────────────────────────────────────────────────────────────────────

describe('extractSignatures — CSS multi-line block comment close', () => {
  it('drops multi-line block comments in CSS that properly close with */', () => {
    const src = `/*
 * Base reset styles.
 * Applied globally.
 */
.container {
  display: flex;
  color: red;
}

/*
 * Typography.
 */
h1,
h2 {
  font-weight: bold;
}
`;
    const sigs = extractSignatures(src, 'styles.css')!;

    expect(sigs).not.toBeNull();
    expect(sigs).not.toContain('Base reset styles');
    expect(sigs).not.toContain('Typography');
    expect(sigs).toContain('.container {');
    expect(sigs).toContain('h1,');
    expect(sigs).toContain('h2 {');
  });

  it('handles CSS with interleaved block comments and rules', () => {
    const src = `@import 'base.css';

/*
 * Component styles.
 */
.button {
  padding: 8px;
}
`;
    const sigs = extractSignatures(src, 'component.scss')!;

    expect(sigs).not.toContain('Component styles');
    expect(sigs).toContain("@import 'base.css'");
    expect(sigs).toContain('.button {');
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 9. extractSignatures — C family enum body skipped (line 725)
//    isEnum=true AND depth>0: body lines NOT pushed, only closing brace kept
// ─────────────────────────────────────────────────────────────────────────────

describe('extractSignatures — C family enum body handling', () => {
  it('keeps enum head and closing brace, drops member lines', () => {
    const src = `#include <stdio.h>

enum Color {
    RED   = 0,
    GREEN = 1,
    BLUE  = 2,
    ALPHA = 3
};

struct Point {
    int x;
    int y;
};
`;
    const sigs = extractSignatures(src, 'types.c')!;

    expect(sigs).not.toBeNull();
    expect(sigs).toContain('#include <stdio.h>');
    expect(sigs).toContain('enum Color {');
    // Member lines inside enum body are dropped (line 725 FALSE branch)
    expect(sigs).not.toContain('RED   = 0');
    expect(sigs).not.toContain('GREEN = 1');
    expect(sigs).not.toContain('BLUE  = 2');
    // Struct members ARE kept (line 725 TRUE branch via !isEnum)
    expect(sigs).toContain('struct Point {');
    expect(sigs).toContain('int x;');
    expect(sigs).toContain('int y;');
  });

  it('keeps typedef enum head and drops body, keeps closing semicolon', () => {
    const src = `typedef enum Status {
    STATUS_OK      = 200,
    STATUS_NOT_FOUND = 404,
    STATUS_ERROR   = 500
} Status;
`;
    const sigs = extractSignatures(src, 'status.h')!;

    expect(sigs).not.toBeNull();
    expect(sigs).toContain('typedef enum Status {');
    expect(sigs).not.toContain('STATUS_OK');
    expect(sigs).not.toContain('STATUS_NOT_FOUND');
  });
});
