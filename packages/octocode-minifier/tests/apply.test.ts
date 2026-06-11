import { describe, it, expect } from 'vitest';
import {
  applyMinification,
  applyContentViewMinification,
} from '@octocodeai/octocode-minifier';

/**
 * Branch-coverage tests for applyMinification and applyContentViewMinification.
 * Tests real behaviour through the public API — no internal module mocking needed
 * because the catch paths are reachable via null/undefined inputs.
 */

describe('applyMinification', () => {
  it('returns minified content when it is shorter', () => {
    const content = `// This is a comment
const   x   =   1;
const   y   =   2;

`;
    const result = applyMinification(content, 'test.ts');
    // Conservative strategy strips comments + trailing whitespace → shorter
    expect(result).not.toContain('// This is a comment');
    expect(result).toContain('const   x   =   1;');
    expect(result).toContain('const   y   =   2;');
    expect(result.length).toBeLessThan(content.length);
  });

  it('is idempotent after it has produced a shorter result', () => {
    const content = '// hidden\nconst x = 1;\n\n\n';
    const once = applyMinification(content, 'test.ts');
    const twice = applyMinification(once, 'test.ts');

    expect(once).toBe('const x = 1;');
    expect(twice).toBe(once);
  });

  it('returns original when minified result is not shorter', () => {
    // Already-compact content — minifier output won't be shorter
    const content = 'x';
    expect(applyMinification(content, 'test.txt')).toBe(content);
  });

  it('returns original on error (null input)', () => {
    // null triggers Buffer.byteLength inside minifyContentSync → exception → catch
    const result = applyMinification(null as unknown as string, 'test.ts');
    expect(result).toBeNull();
  });

  it('handles JSON files correctly', () => {
    const json = '{"a":  1, "b":  2}';
    const result = applyMinification(json, 'config.json');
    // JSON strategy produces compact JSON → shorter
    expect(result).toBe('{"a":1,"b":2}');
  });
});

describe('applyContentViewMinification', () => {
  it('JSON extension path — clean JSON stays readable (same length, original returned)', () => {
    const json = '{\n  "name": "demo",\n  "version": "1.0.0"\n}';
    const result = applyContentViewMinification(json, 'package.json');
    // minifyJsonReadable re-pretty-prints → same length → not shorter → original returned
    expect(result).toContain('"name": "demo"');
    expect(result).toContain('"version": "1.0.0"');
  });

  it('JSON path with already-compact input returns original (not shorter)', () => {
    const json = '{"a":1}';
    const result = applyContentViewMinification(json, 'config.json');
    expect(result).toBe(json);
  });

  it('known comment-strip extension (ts) — removes comments and compresses whitespace', () => {
    const code = `// comment\nconst x = 1;\n\n\nconst y = 2;\n`;
    const result = applyContentViewMinification(code, 'file.ts');
    expect(result).not.toContain('// comment');
    expect(result).toContain('const x = 1;');
    expect(result).toContain('const y = 2;');
    expect(result.length).toBeLessThan(code.length);
  });

  it('unknown extension falls back to general core (whitespace compression)', () => {
    const code = `line1\n\n\n\nline2    \n`;
    const result = applyContentViewMinification(code, 'file.xyz');
    expect(result).not.toMatch(/\n{3,}/);
  });

  it('JSONC/JSON5: strips comments and trailing commas, returns pretty-printed (shorter → returned)', () => {
    const jsonc = `{\n  // Package identity\n  "name": "demo",\n  "scripts": [\n    "test",\n  ],\n}`;
    const result = applyContentViewMinification(jsonc, 'config.jsonc');
    // Comment lines removed → result IS shorter → returned
    expect(result).not.toContain('// Package identity');
    expect(result).toContain('"name": "demo"');
    expect(result).toContain('"test"');
    // Still readable (pretty-printed), not single-line
    expect(result).toContain('\n');
  });

  it('returns original on error (null content)', () => {
    const result = applyContentViewMinification(
      null as unknown as string,
      'file.ts'
    );
    expect(result).toBeNull();
  });

  it('PHP extension uses multiple comment groups before general core', () => {
    const php = `<?php\n/* block */\n# hash\necho "test";\n\n\n?>`;
    const result = applyContentViewMinification(php, 'file.php');
    expect(result).not.toContain('/* block */');
    expect(result).not.toContain('# hash');
  });

  it('returns original when result is not shorter (no room to compress)', () => {
    const compact = 'a=1;b=2;';
    const result = applyContentViewMinification(compact, 'file.xyz');
    expect(result).toBe(compact);
  });

  it('is idempotent after content-view minification removes comments', () => {
    const code = '// comment\nexport const x = 1;\n\n\nexport const y = 2;\n';
    const once = applyContentViewMinification(code, 'file.ts');
    const twice = applyContentViewMinification(once, 'file.ts');

    expect(once).not.toContain('// comment');
    expect(once).toContain('export const x = 1;');
    expect(twice).toBe(once);
  });

  it.each([
    {
      label: 'Markdown',
      filePath: 'README.md',
      content: '# Title\n\n<!-- hidden -->\n\nBody\n',
      expected: '# Title',
      removed: '<!-- hidden -->',
    },
    {
      label: 'SQL',
      filePath: 'schema.sql',
      content: '-- hidden\nCREATE TABLE users (\n  id INT\n);\n',
      expected: 'CREATE TABLE users',
      removed: '-- hidden',
    },
    {
      label: 'Lua',
      filePath: 'init.lua',
      content: '-- hidden\nlocal value = 1\n',
      expected: 'local value = 1',
      removed: '-- hidden',
    },
    {
      label: 'Handlebars',
      filePath: 'view.hbs',
      content: '{{!-- hidden --}}\n<div>{{name}}</div>\n',
      expected: '<div>{{name}}</div>',
      removed: '{{!-- hidden --}}',
    },
    {
      label: 'Haskell',
      filePath: 'Main.hs',
      content: '-- hidden\nmain = putStrLn "hi"\n',
      expected: 'main = putStrLn "hi"',
      removed: '-- hidden',
    },
    {
      label: 'HTML',
      filePath: 'index.html',
      content: '<!-- hidden -->\n<main>\n  <h1>Title</h1>\n</main>\n',
      expected: '<main>',
      removed: '<!-- hidden -->',
    },
  ])('$label path removes comments and preserves useful content', testCase => {
    const result = applyContentViewMinification(
      testCase.content,
      testCase.filePath
    );

    expect(result.trim()).not.toBe('');
    expect(result).toContain(testCase.expected);
    expect(result).not.toContain(testCase.removed);
  });
});
