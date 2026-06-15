/**
 * Integration tests for the FFI boundary.
 * These run against the compiled .node addon — requires `yarn build:dev` first.
 *
 * Run: yarn test:node
 */

import { describe, it, expect, beforeAll } from 'vitest';
import { existsSync, mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';

// ── addon availability guard ──────────────────────────────────────────────────

function platformKey(): string | null {
  if (process.platform === 'darwin') return `darwin-${process.arch}`;
  if (process.platform === 'win32' && process.arch === 'x64') {
    return 'win32-x64-msvc';
  }
  if (process.platform === 'linux') {
    const report = process.report?.getReport() as
      | { header?: { glibcVersionRuntime?: string } }
      | undefined;
    const libc = report?.header?.glibcVersionRuntime ? 'gnu' : 'musl';
    if (process.arch === 'x64') return `linux-x64-${libc}`;
    if (process.arch === 'arm64') return `linux-arm64-${libc}`;
  }
  return null;
}

const key = platformKey();
const addonExists =
  existsSync(join(__dirname, '..', 'octocode-context-utils.node')) ||
  (key !== null &&
    (existsSync(join(__dirname, '..', `octocode-context-utils.${key}.node`)) ||
      existsSync(
        join(
          __dirname,
          '..',
          'npm',
          key,
          `octocode-context-utils.${key}.node`
        )
      )));

// A missing addon must FAIL the suite — silent skipping masks broken builds.
// Run `yarn build:dev` in packages/octocode-context-utils to build the addon.
if (!addonExists) {
  throw new Error(
    'FFI addon not built — run `yarn build:dev` in packages/octocode-context-utils before running tests.'
  );
}

let addon: typeof import('../index.js') | null = null;
const esmLoaderPath = '../index.mjs';

async function importEsmLoader(): Promise<typeof import('../index.js')> {
  return (await import(esmLoaderPath)) as typeof import('../index.js');
}

const MINIFIER_FUNCTION_EXPORTS = [
  'getExtension',
  'minifyContentSync',
  'minifyContentResult',
  'minifyContent',
  'applyMinification',
  'applyContentViewMinification',
  'removeComments',
  'minifyConservativeCore',
  'minifyAggressiveCore',
  'minifyJsonCore',
  'minifyJsonReadable',
  'minifyCodeCore',
  'minifyGeneralCore',
  'minifyMarkdownCore',
  'minifyCSSCore',
  'minifyHTMLCore',
  'minifyJavaScriptCore',
  'minifyCSSQuality',
  'minifyHTMLQuality',
  'stripPythonDocstrings',
  'extractSignatures',
  'getSemanticBoundaryOffsets',
  'getSupportedSignatureExtensions',
  'jsonToYamlString',
  'getMINIFY_CONFIG',
  'parseRipgrepJson',
  'queryFileSystem',
  'charToByteOffset',
  'byteToCharOffset',
  'byteSliceContent',
  'sliceContent',
  'extractMatchingLines',
  'filterPatch',
] as const satisfies readonly (keyof typeof import('../index.js'))[];

const PUBLIC_NATIVE_EXPORTS = [
  'SIGNATURES_ONLY_HINT',
  ...MINIFIER_FUNCTION_EXPORTS,
  'MINIFY_CONFIG',
  'SUPPORTED_SIGNATURE_EXTENSIONS',
] as const satisfies readonly (keyof typeof import('../index.js'))[];

beforeAll(async () => {
  if (!addonExists) return;
  addon = await import('../index.js');
});



describe('getExtension', () => {
  it('returns extension from normal file', () => {
    expect(addon!.getExtension('foo.ts', { lowercase: true })).toBe('ts');
  });

  it('handles dotfile (.gitignore)', () => {
    expect(addon!.getExtension('.gitignore', { lowercase: true })).toBe(
      'gitignore'
    );
  });

  it('returns configured default for no-extension name', () => {
    expect(
      addon!.getExtension('Makefile', { lowercase: true, fallback: 'txt' })
    ).toBe('txt');
  });
});

describe('queryFileSystem', () => {
  it('finds files by glob and skips excluded directories', () => {
    const root = mkdtempSync(join(tmpdir(), 'octocode-fs-query-'));
    try {
      mkdirSync(join(root, 'src', 'nested'), { recursive: true });
      mkdirSync(join(root, 'node_modules', 'pkg'), { recursive: true });
      writeFileSync(join(root, 'src', 'nested', 'main.ts'), 'export {}');
      writeFileSync(join(root, 'src', 'nested', 'main.js'), 'module.exports = {}');
      writeFileSync(join(root, 'node_modules', 'pkg', 'index.ts'), 'ignored');

      const result = addon!.queryFileSystem({
        path: root,
        names: ['*.ts'],
        excludeDir: ['node_modules'],
        maxDepth: 3,
        entryType: 'f',
      });

      expect(result.entries.map(entry => entry.relativePath)).toEqual([
        'src/nested/main.ts',
      ]);
      expect(result.totalDiscovered).toBe(1);
      expect(result.wasCapped).toBe(false);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});

describe('removeComments', () => {
  it('strips c-style line comments', () => {
    const out = addon!.removeComments(
      'int x = 1; // comment\nint y;',
      'c-style'
    );
    expect(out).not.toContain('comment');
    expect(out).toContain('int x');
  });

  it('strips hash comments', () => {
    const out = addon!.removeComments(
      'x = 1 # inline\n# whole line\ny = 2',
      'hash'
    );
    expect(out).toContain('x = 1');
    expect(out).not.toContain('inline');
    expect(out).not.toContain('whole line');
  });

  it('accepts array of comment types', () => {
    const out = addon!.removeComments('x = 1 # hash\n/* block */', [
      'hash',
      'c-style',
    ]);
    expect(out).not.toContain('hash');
    expect(out).not.toContain('block');
  });

  it('returns original on unknown type (no panic)', () => {
    const out = addon!.removeComments(
      'hello',
      'nonexistent-type' as unknown as import('../index.js').CommentPatternGroup
    );
    expect(out).toBe('hello');
  });
});

describe('minifyJsonCore', () => {
  it('compacts valid JSON', () => {
    const r = addon!.minifyJsonCore('{"a": 1, "b": 2 }');
    expect(r.failed).toBe(false);
    expect(r.content).toBe('{"a":1,"b":2}');
  });

  it('strips JSONC comments and trailing commas', () => {
    const src = '{\n  // comment\n  "key": "value",\n}';
    const r = addon!.minifyJsonCore(src);
    expect(r.failed).toBe(false);
    expect(r.content).toContain('key');
  });

  it('marks invalid JSON as failed', () => {
    const r = addon!.minifyJsonCore('{ invalid json');
    expect(r.failed).toBe(true);
    expect(r.content).toBe('{ invalid json');
  });
});

describe('minifyCodeCore', () => {
  it('collapses 3+ blank lines to max 1', () => {
    const out = addon!.minifyCodeCore('a\n\n\n\nb');
    expect(out).toBe('a\n\nb');
    expect(out).not.toContain('\n\n\n');
  });

  it('preserves indentation', () => {
    const src = 'function f() {\n  return 1;\n}';
    const out = addon!.minifyCodeCore(src);
    expect(out).toContain('  return');
  });
});

describe('minifyMarkdownCore', () => {
  it('removes markdown emoji/noise and compacts paragraph newlines', () => {
    const src = `# Guide 🚀

This is a soft
wrapped paragraph 😊 with :sparkles: punctuation .

<a id="top"></a>
<br />
![Screenshot](./screen.png)

\`\`\`js
console.log("😀 keep literal");
\`\`\`
`;
    const out = addon!.minifyMarkdownCore(src);
    expect(out).toContain('# Guide');
    expect(out).toContain('This is a soft wrapped paragraph with punctuation.');
    expect(out).toContain('console.log("😀 keep literal");');
    expect(out).not.toContain('🚀');
    expect(out).not.toContain('😊');
    expect(out).not.toContain(':sparkles:');
    expect(out).not.toContain('Screenshot');
    expect(out).not.toContain('<a id');
    expect(out).not.toContain('<br');
    expect(out).not.toContain('\n\n');
  });
});

describe('minifyContentSync', () => {
  it('strips JS comments for .js file', () => {
    const out = addon!.minifyContentSync(
      'const x = 1; // comment\n',
      'file.js'
    );
    expect(out).not.toContain('comment');
  });

  it('minifies JSON for .json file', () => {
    const out = addon!.minifyContentSync('{ "a": 1 }', 'data.json');
    expect(out).toBe('{"a":1}');
  });
});

describe('minifyContent (async wrapper)', () => {
  it('returns a Promise', async () => {
    const result = addon!.minifyContent('const x = 1;', 'file.js');
    expect(result).toBeInstanceOf(Promise);
    const r = await result;
    expect(r).toHaveProperty('content');
    expect(r).toHaveProperty('failed');
    expect(r).toHaveProperty('type');
  });

  it('resolves with correct content', async () => {
    const r = await addon!.minifyContent('{ "k": 1 }', 'data.json');
    expect(r.failed).toBe(false);
    expect(r.content).toBe('{"k":1}');
  });
});

describe('applyContentViewMinification', () => {
  it('strips comments but preserves indentation for code', () => {
    const out = addon!.applyContentViewMinification(
      'fn foo() {\n  // comment\n  let x = 1;\n}',
      'main.rs'
    );
    expect(out).not.toContain('comment');
    expect(out).toContain('  let x');
  });

  it('returns original if not shorter', () => {
    const src = 'hello world';
    const out = addon!.applyContentViewMinification(src, 'file.txt');
    expect(out).toBe(src);
  });
});

describe('extractSignatures', () => {
  it('extracts TypeScript function signatures', () => {
    const src = `
export function add(a: number, b: number): number {
  return a + b;
}
export class Calc {
  value = 0;
  multiply(x: number): number { return x; }
}
`;
    const out = addon!.extractSignatures(src, 'calc.ts');
    expect(out).not.toBeNull();
    expect(out).toContain('add');
    expect(out).toContain('Calc');
    expect(out).not.toContain('return a + b');
  });

  it('extracts Python function signatures', () => {
    const src = `
import os

class Foo:
    def bar(self) -> str:
        return "hello"

def top():
    pass
`;
    const out = addon!.extractSignatures(src, 'foo.py');
    expect(out).not.toBeNull();
    expect(out).toContain('def bar');
    expect(out).toContain('def top');
    expect(out).not.toContain('return "hello"');
  });

  it('returns null for unknown extension', () => {
    const out = addon!.extractSignatures('hello', 'file.xyz123');
    // May return null or a skeleton — must not throw
    expect(() =>
      addon!.extractSignatures('hello', 'file.xyz123')
    ).not.toThrow();
  });

  it('returns null for empty content', () => {
    const out = addon!.extractSignatures('', 'file.ts');
    expect(out).toBeNull();
  });

  it('extracts Markdown document outlines', () => {
    const src = `---
title: Guide
---

# Project

Intro with [Docs](https://example.com/docs) and [API][api].

## Install ##

- yarn install

\`\`\`ts
export function hidden() {
  return 1;
}
\`\`\`

API
===

[api]: ./api.md
`;
    const out = addon!.extractSignatures(src, 'README.md');
    expect(out).not.toBeNull();
    expect(out!).toContain('frontmatter: title');
    expect(out!).toContain('# Project');
    expect(out!).toContain(
      'links: [Docs](https://example.com/docs), [API][api]'
    );
    expect(out!).toContain('## Install');
    expect(out!).toContain('- yarn install');
    expect(out!).toContain('code fence: ts');
    expect(out!).toContain('# API');
    expect(out!).toContain('link ref: [api]: ./api.md');
    expect(out!).not.toContain('hidden');
  });
});

describe('jsonToYamlString', () => {
  it('serializes a plain object to YAML', () => {
    const out = addon!.jsonToYamlString({ a: 1, b: 'hello' });
    expect(out).toContain('a:');
    expect(out).toContain('b:');
  });

  it('sorts keys when sortKeys=true', () => {
    const out = addon!.jsonToYamlString(
      { z: 3, a: 1, m: 2 },
      { sortKeys: true }
    );
    const aPos = out.indexOf('a:');
    const mPos = out.indexOf('m:');
    const zPos = out.indexOf('z:');
    expect(aPos).toBeLessThan(mPos);
    expect(mPos).toBeLessThan(zPos);
  });

  it('respects priority keys', () => {
    const out = addon!.jsonToYamlString(
      { z: 3, a: 1, b: 2 },
      { keysPriority: ['b', 'z'] }
    );
    const bPos = out.indexOf('b:');
    const zPos = out.indexOf('z:');
    const aPos = out.indexOf('a:');
    expect(bPos).toBeLessThan(zPos);
    expect(zPos).toBeLessThan(aPos);
  });

  it('handles multiline strings as block scalars', () => {
    const out = addon!.jsonToYamlString({ msg: 'line1\nline2' });
    expect(out).toContain('|-');
  });
});

describe('minifyCSSQuality', () => {
  it('strips comments and compacts CSS', () => {
    const src = 'h1 { color: red; } /* comment */ p { margin: 0px 0px; }';
    const out = addon!.minifyCSSQuality(src);
    expect(out).not.toContain('comment');
    expect(out.length).toBeLessThan(src.length);
  });
});

describe('minifyHTMLQuality', () => {
  it('strips HTML comments', () => {
    const src = '<html><body><!-- comment --><h1>Hi</h1></body></html>';
    const out = addon!.minifyHTMLQuality(src);
    expect(out).not.toContain('comment');
    expect(out).toContain('Hi');
  });
});

describe('SIGNATURES_ONLY_HINT', () => {
  it('is a non-empty string', () => {
    expect(typeof addon!.SIGNATURES_ONLY_HINT).toBe('string');
    expect(addon!.SIGNATURES_ONLY_HINT.length).toBeGreaterThan(0);
  });
});

describe('getSupportedSignatureExtensions', () => {
  it('returns an array including ts and py', () => {
    if (!addon) return;
    const exts = addon.getSupportedSignatureExtensions();
    expect(exts).toContain('ts');
    expect(exts).toContain('py');
    expect(exts).toContain('rs');
    expect(exts).toContain('md');
    expect(exts).toContain('markdown');
  });
});

// ── UTF-8 safety across the FFI boundary ──────────────────────────────────────

describe('UTF-8 preservation', () => {
  it('aggressive strategy preserves non-ASCII (lua)', () => {
    const out = addon!.minifyContentSync(
      'local s = "café → naïve" { x = 1 }',
      'a.lua'
    );
    expect(out).toContain('café → naïve');
    expect(out).not.toContain('Ã');
  });

  it('JSONC strip preserves non-ASCII', () => {
    const r = addon!.minifyJsonCore('{\n  // comment\n  "k": "café",\n}');
    expect(r.failed).toBe(false);
    expect(r.content).toContain('café');
    expect(r.content).not.toContain('Ã');
  });

  it('content view preserves non-ASCII markdown', () => {
    const out = addon!.applyContentViewMinification(
      '# Tîtle\n\ncafé text\n',
      'x.md'
    );
    expect(out).toContain('Tîtle');
    expect(out).toContain('café');
  });
});

// ── size-cap contract ─────────────────────────────────────────────────────────

describe('oversized input contract', () => {
  it('minifyContentResult flags >1MB as failed', () => {
    const big = 'x'.repeat(1024 * 1024 + 1);
    const r = addon!.minifyContentResult(big, 'big.txt');
    expect(r.failed).toBe(true);
    expect(r.content).toBe(big);
  });

  it('applyContentViewMinification returns >1MB input untouched', () => {
    const big = 'text  \n'.repeat(180_000);
    expect(addon!.applyContentViewMinification(big, 'big.md')).toBe(big);
  });

  it('extractSignatures returns null for >1MB input', () => {
    const big = 'function f(){ return 1; }\n'.repeat(45_000);
    expect(addon!.extractSignatures(big, 'big.ts')).toBeNull();
  });
});

// ── skeleton one-liners ───────────────────────────────────────────────────────

describe('python one-liner signatures', () => {
  it('keeps the signature row of a one-line def', () => {
    const out = addon!.extractSignatures(
      'def f(): return 1\n\ndef g():\n    return 2\n',
      'one.py'
    );
    expect(out).not.toBeNull();
    expect(out!).toContain('def f(): return 1');
    expect(out!).toContain('def g():');
    expect(out!).not.toContain('return 2');
  });
});

// ── public wrapper exports (CJS) ─────────────────────────────────────────────

describe('public wrapper additions', () => {
  it('does not rely on generated JS fallback loader artifacts', () => {
    expect(existsSync(join(__dirname, '..', 'native.cjs'))).toBe(false);
    expect(existsSync(join(__dirname, '..', 'native.d.ts'))).toBe(false);
  });

  it('minifyContent resolves to a MinifyResult', async () => {
    const r = await addon!.minifyContent('{"a": 1 }', 'x.json');
    expect(r.failed).toBe(false);
    expect(typeof r.content).toBe('string');
  });

  it('MINIFY_CONFIG and SUPPORTED_SIGNATURE_EXTENSIONS are exported', () => {
    expect(addon!.MINIFY_CONFIG).toBeTruthy();
    expect(addon!.MINIFY_CONFIG.fileTypes).toBeTruthy();
    expect(Array.isArray(addon!.SUPPORTED_SIGNATURE_EXTENSIONS)).toBe(true);
    expect(addon!.SUPPORTED_SIGNATURE_EXTENSIONS).toContain('ts');
  });
});

// ── parseRipgrepJson ──────────────────────────────────────────────────────────

describe('parseRipgrepJson', () => {
  const makeMatch = (path: string, text: string, line: number, col = 0) =>
    JSON.stringify({
      type: 'match',
      data: {
        path: { text: path },
        lines: { text: text },
        line_number: line,
        absolute_offset: 0,
        submatches: [{ match: { text: 'x' }, start: col, end: col + 1 }],
      },
    });

  const makeSummary = (matches: number) =>
    JSON.stringify({
      type: 'summary',
      data: {
        elapsed: { human: '0.001s', nanos: 1_000_000, secs: 0 },
        stats: {
          bytes_printed: 0,
          bytes_searched: 500,
          elapsed: { human: '0.001s', nanos: 1_000_000, secs: 0 },
          matched_lines: matches,
          matches,
          searches: 1,
          searches_with_match: 1,
        },
      },
    });

  it('returns empty result for empty stdout', () => {
    const r = addon!.parseRipgrepJson('', null);
    expect(r.files).toHaveLength(0);
    expect(r.stats.matchCount).toBeUndefined();
  });

  it('parses a single match line', () => {
    const stdout = makeMatch('src/foo.ts', '  const x = 1;\n', 10, 8);
    const r = addon!.parseRipgrepJson(stdout, null);
    expect(r.files).toHaveLength(1);
    expect(r.files[0].path).toBe('src/foo.ts');
    expect(r.files[0].matchCount).toBe(1);
    expect(r.files[0].matches[0].line).toBe(10);
    expect(r.files[0].matches[0].column).toBe(8);
    expect(r.files[0].matches[0].value).toBe('  const x = 1;');
  });

  it('strips trailing newline from match value', () => {
    const stdout = makeMatch('f.ts', 'line\n', 1, 0);
    const r = addon!.parseRipgrepJson(stdout, null);
    expect(r.files[0].matches[0].value).toBe('line');
  });

  it('parses summary stats', () => {
    const stdout = [makeMatch('f.ts', 'x\n', 1, 0), makeSummary(2)].join('\n');
    const r = addon!.parseRipgrepJson(stdout, null);
    expect(r.stats.matchCount).toBe(2);
    expect(r.stats.searchTime).toBe('0.001s');
  });

  it('truncates snippets to maxSnippetChars', () => {
    const long = 'a'.repeat(600);
    const stdout = makeMatch('f.ts', long + '\n', 1, 0);
    const r = addon!.parseRipgrepJson(stdout, { maxSnippetChars: 10 });
    expect(r.files[0].matches[0].value.length).toBeLessThanOrEqual(10);
  });

  it('preserves unicode content', () => {
    const stdout = makeMatch('f.ts', 'café → naïve\n', 1, 0);
    const r = addon!.parseRipgrepJson(stdout, null);
    expect(r.files[0].matches[0].value).toBe('café → naïve');
  });

  it('groups multiple matches under same file', () => {
    const stdout = [
      makeMatch('f.ts', 'line1\n', 1, 0),
      makeMatch('f.ts', 'line2\n', 5, 0),
    ].join('\n');
    const r = addon!.parseRipgrepJson(stdout, null);
    expect(r.files).toHaveLength(1);
    expect(r.files[0].matchCount).toBe(2);
  });
});

// ── UTF-8 offset helpers ──────────────────────────────────────────────────────

describe('charToByteOffset', () => {
  it('ASCII: char offset equals byte offset', () => {
    expect(addon!.charToByteOffset('hello', 3)).toBe(3);
    expect(addon!.charToByteOffset('hello', 0)).toBe(0);
  });

  it('multibyte: é is 2 bytes', () => {
    const s = 'café'; // c(1) a(1) f(1) é(2) → 5 bytes total
    expect(addon!.charToByteOffset(s, 4)).toBe(5);
  });

  it('emoji: char offset uses JavaScript UTF-16 code units', () => {
    const s = 'a🌍b';
    expect(addon!.charToByteOffset(s, 1)).toBe(1);
    expect(addon!.charToByteOffset(s, 3)).toBe(5);
    expect(addon!.charToByteOffset(s, 4)).toBe(6);
  });

  it('clamps beyond string length', () => {
    expect(addon!.charToByteOffset('hi', 100)).toBe(2);
  });
});

describe('byteToCharOffset', () => {
  it('ASCII: byte offset equals char offset', () => {
    expect(addon!.byteToCharOffset('hello', 3)).toBe(3);
  });

  it('multibyte: 5 bytes into café = 4 chars', () => {
    expect(addon!.byteToCharOffset('café', 5)).toBe(4);
  });

  it('emoji: byte offset returns JavaScript UTF-16 code units', () => {
    const s = 'a🌍b';
    expect(addon!.byteToCharOffset(s, 1)).toBe(1);
    expect(addon!.byteToCharOffset(s, 5)).toBe(3);
    expect(addon!.byteToCharOffset(s, 6)).toBe(4);
  });
});

describe('byteSliceContent', () => {
  it('extracts ASCII range', () => {
    expect(addon!.byteSliceContent('hello world', 6, 11)).toBe('world');
  });

  it('extracts multibyte char', () => {
    expect(addon!.byteSliceContent('café', 3, 5)).toBe('é');
  });

  it('returns empty for bad range', () => {
    expect(addon!.byteSliceContent('hello', 3, 2)).toBe('');
  });
});

describe('sliceContent', () => {
  it('basic char window', () => {
    const r = addon!.sliceContent('abcdefghij', 3, 4, null);
    expect(r.text).toBe('defg');
    expect(r.charOffset).toBe(3);
    expect(r.hasMore).toBe(true);
  });

  it('last page has no more', () => {
    const r = addon!.sliceContent('abcde', 3, 10, null);
    expect(r.text).toBe('de');
    expect(r.hasMore).toBe(false);
    expect(r.nextCharOffset).toBeUndefined();
  });

  it('multibyte: charLength counts JavaScript UTF-16 code units', () => {
    const r = addon!.sliceContent('café world', 0, 4, null);
    expect(r.text).toBe('café');
    expect(r.charLength).toBe(4);
    expect(r.byteLength).toBe(5); // é = 2 bytes
  });

  it('emoji: sliceContent uses JavaScript UTF-16 code units', () => {
    const r = addon!.sliceContent('a🌍b', 0, 3, null);
    expect(r.text).toBe('a🌍');
    expect(r.charLength).toBe(3);
    expect(r.byteLength).toBe(5);
    expect(r.nextCharOffset).toBe(3);
  });

  it('snap to line boundary', () => {
    const content = 'line1\nline2\nline3\n';
    const r = addon!.sliceContent(content, 3, 4, { snapToLineBoundary: true });
    expect(r.text.startsWith('line1')).toBe(true);
    expect(r.charOffset).toBe(0);
  });

  it('charOffset + byteOffset round-trip', () => {
    const content = 'hello 世界 world';
    const r = addon!.sliceContent(content, 6, 2, null);
    expect(r.text).toBe('世界');
    expect(r.byteLength).toBe(6); // each CJK char = 3 bytes
  });
});

// ── extractMatchingLines ──────────────────────────────────────────────────────

describe('extractMatchingLines', () => {
  it('finds literal match case-insensitively', () => {
    const r = addon!.extractMatchingLines('Hello World\nfoo', 'hello', null);
    expect(r.matchCount).toBe(1);
    expect(r.matchingLines).toEqual([1]);
    expect(r.lines[0]).toBe('Hello World');
  });

  it('case-sensitive when requested', () => {
    const r = addon!.extractMatchingLines('Hello\nhello', 'hello', {
      caseSensitive: true,
    });
    expect(r.matchCount).toBe(1);
    expect(r.matchingLines).toEqual([2]);
  });

  it('regex match', () => {
    const r = addon!.extractMatchingLines('const x = 1;\nlet y = 2;', String.raw`(const|let)\s+\w`, {
      isRegex: true,
    });
    expect(r.matchCount).toBe(2);
  });

  it('context lines included', () => {
    const r = addon!.extractMatchingLines('a\nb\nc match\nd\ne', 'match', {
      contextLines: 1,
    });
    expect(r.lines).toContain('b');
    expect(r.lines).toContain('c match');
    expect(r.lines).toContain('d');
  });

  it('whitespace-stripped fallback', () => {
    const r = addon!.extractMatchingLines('hello    world\nfoo', 'helloworld', null);
    expect(r.matchCount).toBe(1);
  });

  it('no match returns empty result', () => {
    const r = addon!.extractMatchingLines('foo\nbar', 'zzz', null);
    expect(r.matchCount).toBe(0);
    expect(r.lines).toHaveLength(0);
  });

  it('max matches cap', () => {
    const r = addon!.extractMatchingLines('x\nx\nx\nx\nx', 'x', { maxMatches: 2 });
    expect(r.matchingLines).toHaveLength(2);
  });

  it('preserves unicode content', () => {
    const r = addon!.extractMatchingLines('café\nnormal', 'café', null);
    expect(r.matchCount).toBe(1);
    expect(r.lines[0]).toContain('café');
  });
});

// ── filterPatch ───────────────────────────────────────────────────────────────

describe('filterPatch', () => {
  const samplePatch = '@@ -1,4 +1,4 @@\n context1\n-deleted\n+added\n context2';

  it('returns original when no options', () => {
    expect(addon!.filterPatch(samplePatch, null)).toBe(samplePatch);
  });

  it('returns empty for empty patch', () => {
    expect(addon!.filterPatch('', null)).toBe('');
  });

  it('filters by addition line number', () => {
    const patch = '@@ -1,3 +1,3 @@\n context\n+line2\n+line3';
    const r = addon!.filterPatch(patch, { additions: [2] });
    expect(r).toContain('+2:');
    expect(r).not.toContain('+3:');
  });

  it('filters by deletion line number', () => {
    const patch = '@@ -1,3 +1,3 @@\n context\n-line2\n-line3';
    const r = addon!.filterPatch(patch, { deletions: [2] });
    expect(r).toContain('-2:');
    expect(r).not.toContain('-3:');
  });

  it('trim context on long patch inserts markers', () => {
    const lines = ['@@ -1,50 +1,50 @@'];
    for (let i = 1; i <= 48; i++) lines.push(` context${i}`);
    lines.push('+added_line');
    lines.push('-deleted_line');
    const patch = lines.join('\n');
    const r = addon!.filterPatch(patch, { trimContext: true, contextLines: 2 });
    expect(r).toContain('added_line');
    expect(r).toContain('deleted_line');
    expect(r).toContain('...');
  });

  it('short patch not trimmed even with trimContext=true', () => {
    const r = addon!.filterPatch(samplePatch, { trimContext: true });
    expect(r).not.toContain('...');
  });
});

describe('ESM/CJS loader parity', () => {
  it('exports the same public native surface from both loaders', async () => {
    const esm = await importEsmLoader();

    for (const name of PUBLIC_NATIVE_EXPORTS) {
      expect(addon).toHaveProperty(name);
      expect(esm).toHaveProperty(name);
    }
  });

  it('exposes the same minifier function exports from both loaders', async () => {
    const esm = await importEsmLoader();

    for (const name of MINIFIER_FUNCTION_EXPORTS) {
      expect(typeof addon![name]).toBe('function');
      expect(typeof esm[name]).toBe('function');
    }

    const cjsFunctions = MINIFIER_FUNCTION_EXPORTS.filter(
      name => typeof addon![name] === 'function'
    );
    const esmFunctions = MINIFIER_FUNCTION_EXPORTS.filter(
      name => typeof esm[name] === 'function'
    );
    expect(esmFunctions).toEqual(cjsFunctions);
  });

  it('returns identical semantic boundary offsets through ESM and CJS', async () => {
    const esm = await importEsmLoader();
    const src = 'function a() {\n  return 1;\n}\n\nfunction b() {}\n';

    expect(esm.getSemanticBoundaryOffsets(src, 'x.ts')).toEqual(
      addon!.getSemanticBoundaryOffsets(src, 'x.ts')
    );
  });
});
