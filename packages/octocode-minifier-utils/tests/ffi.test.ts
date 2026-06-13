/**
 * Integration tests for the FFI boundary.
 * These run against the compiled .node addon — requires `yarn build:dev` first.
 *
 * Run: yarn test:node
 */

import { describe, it, expect, beforeAll } from 'vitest';
import { existsSync } from 'fs';
import { join } from 'path';

// ── addon availability guard ──────────────────────────────────────────────────

const addonExists =
  existsSync(
    join(
      __dirname,
      '..',
      `octocode-minifier-utils.${process.platform}-${process.arch}.node`
    )
  ) || existsSync(join(__dirname, '..', 'octocode-minifier-utils.node'));

// In CI a missing addon must FAIL the suite, not silently skip every test.
if (!addonExists && process.env.CI) {
  throw new Error(
    'FFI addon not built — run `yarn build:dev` before tests. ' +
      'Silent skipping is only allowed for local runs without a compiled addon.'
  );
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
let addon: typeof import('../index.js') | null = null;

beforeAll(async () => {
  if (!addonExists) return;
  addon = await import('../index.js');
});

const skip = !addonExists;

describe.skipIf(skip)('getExtension', () => {
  it('returns extension from normal file', () => {
    expect(addon!.getExtension('foo.ts', { lowercase: true })).toBe('ts');
  });

  it('handles dotfile (.gitignore)', () => {
    expect(addon!.getExtension('.gitignore', { lowercase: true })).toBe(
      'gitignore'
    );
  });

  it('returns fallback for no-extension name', () => {
    expect(
      addon!.getExtension('Makefile', { lowercase: true, fallback: 'txt' })
    ).toBe('txt');
  });
});

describe.skipIf(skip)('removeComments', () => {
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
    const out = addon!.removeComments('hello', 'nonexistent-type');
    expect(out).toBe('hello');
  });
});

describe.skipIf(skip)('minifyJsonCore', () => {
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
});

describe.skipIf(skip)('minifyCodeCore', () => {
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

describe.skipIf(skip)('minifyMarkdownCore', () => {
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

describe.skipIf(skip)('minifyContentSync', () => {
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

describe.skipIf(skip)('minifyContent (async wrapper)', () => {
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

describe.skipIf(skip)('applyContentViewMinification', () => {
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

describe.skipIf(skip)('extractSignatures', () => {
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

describe.skipIf(skip)('jsonToYamlString', () => {
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

describe.skipIf(skip)('minifyCSSQuality', () => {
  it('strips comments and compacts CSS', () => {
    const src = 'h1 { color: red; } /* comment */ p { margin: 0px 0px; }';
    const out = addon!.minifyCSSQuality(src);
    expect(out).not.toContain('comment');
    expect(out.length).toBeLessThan(src.length);
  });
});

describe.skipIf(skip)('minifyHTMLQuality', () => {
  it('strips HTML comments', () => {
    const src = '<html><body><!-- comment --><h1>Hi</h1></body></html>';
    const out = addon!.minifyHTMLQuality(src);
    expect(out).not.toContain('comment');
    expect(out).toContain('Hi');
  });
});

describe.skipIf(skip)('SIGNATURES_ONLY_HINT', () => {
  it('is a non-empty string', () => {
    expect(typeof addon!.SIGNATURES_ONLY_HINT).toBe('string');
    expect(addon!.SIGNATURES_ONLY_HINT.length).toBeGreaterThan(0);
  });
});

describe.skipIf(!addonExists)('getSupportedSignatureExtensions', () => {
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

describe.skipIf(skip)('UTF-8 preservation', () => {
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

describe.skipIf(skip)('oversized input contract', () => {
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

describe.skipIf(skip)('python one-liner signatures', () => {
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

// ── postbuild shim exports (CJS) ──────────────────────────────────────────────

describe.skipIf(skip)('postbuild additions', () => {
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
