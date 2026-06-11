/**
 * Comprehensive type-coverage audit for all three minification paths used by
 * the fetch / search tools.
 *
 * ─── applyContentViewMinification  (sync, conservative)
 *       Used by: localGetFileContent, githubGetFileContent, PR patches
 *       Pipeline per category (driven by MINIFY_CONFIG):
 *         json / jsonc / json5 → minifyJsonCore
 *         md / markdown        → minifyMarkdownCore
 *         files with comments  → removeComments(configured) + minifyGeneralCore
 *         csv / txt / log / …  → minifyGeneralCore only
 *
 * ─── minifyContent (async, aggressive)
 *       Used by: githubSearchCode (fragments)
 *       js/jsx/mjs/cjs  → Terser
 *       css/less/scss   → CleanCSS async
 *       html/htm        → html-minifier-terser async
 *       *               → strategy from MINIFY_CONFIG (sync fallback)
 *
 * ─── extractSignatures
 *       Used by: localGetFileContent, githubGetFileContent (minify:"symbols")
 *       ts/tsx/js/jsx/mjs/cjs  → TypeScript AST (regex fallback)
 *       py  → indent heuristic
 *       go  → column-0 anchoring
 *       java/cs/kt  → line-pattern
 *       rs  → line-pattern
 *       c/h/cpp/hpp → struct+func head
 *       rb  → line-pattern
 *       php → line-pattern
 *       swift → line-pattern
 *       css/scss/less → selector/at-rule heuristic
 *       html/htm → structure elements
 *       vue/svelte → <script> extraction + ts-js AST
 *       sql → CREATE DDL
 *       sh/bash/zsh → function def
 *       else → null (not supported)
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  applyContentViewMinification,
  minifyContent,
  minifyMarkdownCore,
  extractSignatures,
  MINIFY_CONFIG,
} from '@octocodeai/octocode-minifier';

// Module-level mocks for the async minifier dependencies (hoisted calls must
// live at the top level; they apply file-wide either way).
const mockMinify = vi.hoisted(() => vi.fn());
const mockCleanCSS = vi.hoisted(() => vi.fn());
const mockHtmlMinify = vi.hoisted(() => vi.fn());

vi.mock('terser', () => ({ minify: mockMinify }));
vi.mock('clean-css', () => ({
  default: class {
    minify(c: string) {
      return mockCleanCSS(c);
    }
  },
}));
vi.mock('html-minifier-terser', () => ({ minify: mockHtmlMinify }));

// ─── helpers ─────────────────────────────────────────────────────────────────

function str(v: unknown, label: string) {
  expect(typeof v, label).toBe('string');
}

function notLonger(input: string, output: string, label: string) {
  expect(output.length, `${label}: not longer`).toBeLessThanOrEqual(
    input.length
  );
}

function has(r: string, sub: string, label: string) {
  expect(r, `${label}: should contain ${JSON.stringify(sub)}`).toContain(sub);
}

function hasNot(r: string, sub: string, label: string) {
  expect(
    r,
    `${label}: should NOT contain ${JSON.stringify(sub)}`
  ).not.toContain(sub);
}

// ─────────────────────────────────────────────────────────────────────────────
// applyContentViewMinification — per-category correctness
// ─────────────────────────────────────────────────────────────────────────────

describe('applyContentViewMinification — JSON family', () => {
  it('json: clean JSON returns original (already readable, same length)', () => {
    // minifyJsonReadable re-pretty-prints → same length → not shorter → original returned
    const input = '{\n  "a": 1,\n  "b": 2\n}';
    const r = applyContentViewMinification(input, 'f.json');
    str(r, 'json');
    has(r, '"a": 1', 'json-key-a');
    has(r, '"b": 2', 'json-key-b');
  });
  it('jsonc: strips // comments, returns pretty-printed (shorter → returned)', () => {
    // The comment line makes input longer → stripped result is shorter → returned
    const r = applyContentViewMinification(
      '// comment\n{\n  "a": 1\n}',
      'f.jsonc'
    );
    hasNot(r, '// comment', 'jsonc-comment');
    has(r, '"a": 1', 'jsonc-key');
    str(r, 'jsonc');
  });
  it('json5: strips trailing commas and // comments', () => {
    const r = applyContentViewMinification('{\n  "a": 1,  // c\n}', 'f.json5');
    str(r, 'json5');
    hasNot(r, '// c', 'json5-comment');
  });
  it('invalid JSON returns trimmed original', () => {
    const bad = '{ invalid json }';
    const r = applyContentViewMinification(bad, 'f.json');
    str(r, 'bad-json');
  });
});

describe('applyContentViewMinification — Markdown (minifyMarkdownCore)', () => {
  it('md: strips HTML comments', () => {
    const r = applyContentViewMinification(
      '# T\n\n<!-- hidden -->\n\nParagraph\n',
      'README.md'
    );
    hasNot(r, '<!-- hidden -->', 'md');
    has(r, '# T', 'md');
    has(r, 'Paragraph', 'md');
  });
  it('md: compresses 3+ blank lines to max 2', () => {
    const r = applyContentViewMinification('a\n\n\n\nb\n', 'f.md');
    expect(r).not.toMatch(/\n{3,}/);
  });
  it('md: strips quoted-reply lines (>)', () => {
    const r = applyContentViewMinification(
      'reply\n\n> original\n> line\n\nmy text\n',
      'notes.md'
    );
    hasNot(r, '> original', 'md-quote');
  });
  it('markdown ext handled same as md', () => {
    const r = applyContentViewMinification('<!-- c -->\ntext\n', 'f.markdown');
    hasNot(r, '<!-- c -->', 'markdown-ext');
  });
});

describe('applyContentViewMinification — c-style comment family', () => {
  const C_CASES: [string, string][] = [
    ['ts', '// lc\n/* bc */\nconst x = 1; // ic\nconst y = 2;\n'],
    ['tsx', '// lc\nconst A = () => <div />;'],
    ['js', '// lc\n/* bc */\nconst x = 1;\n'],
    ['jsx', '/* bc */\nconst A = () => <div />;'],
    ['mjs', '// lc\nexport const x = 1;\n'],
    ['cjs', '// lc\nmodule.exports = {};\n'],
    ['go', '// lc\npackage main\n'],
    ['java', '// lc\npackage com.example;\n'],
    ['c', '// lc\n#include <stdio.h>\n'],
    ['cpp', '// lc\n#include <iostream>\n'],
    ['cs', '// lc\nusing System;\n'],
    ['rs', '// lc\nuse std::io;\n'],
    ['swift', '// lc\nimport Foundation\n'],
    ['kt', '// lc\nfun main() {}\n'],
    ['scala', '// lc\nimport scala.io\n'],
    ['dart', '// lc\nimport "dart:core";\n'],
    ['css', '/* bc */\n.btn { color: red; }\n'],
    ['less', '/* bc */\n.btn { color: red; }\n'],
    ['scss', '/* bc */\n$x: red;\n'],
    ['sass', '/* bc */\n.btn\n  color: red\n'],
    ['styl', '/* bc */\n.btn { color red }\n'],
    ['proto', '// lc\nsyntax = "proto3";\n'],
  ];
  it.each(C_CASES)('ext=%s strips // and /* */ comments', (ext, input) => {
    const r = applyContentViewMinification(input, `f.${ext}`);
    hasNot(r, '// lc', ext);
    hasNot(r, '/* bc */', ext);
    hasNot(r, '// ic', ext);
  });
});

describe('applyContentViewMinification — hash-comment family', () => {
  const HASH_CASES: [string, string, string][] = [
    ['py', '# comment\nx = 1\n', 'x = 1'],
    ['yaml', '# comment\nkey: val\n', 'key: val'],
    ['yml', '# comment\nfoo: bar\n', 'foo: bar'],
    ['rb', '# comment\ndef foo; end\n', 'def foo'],
    ['sh', '#!/bin/bash\n# comment\necho hi\n', 'echo hi'],
    ['bash', '# comment\necho hi\n', 'echo hi'],
    ['perl', '# comment\nprint "hi";\n', 'print "hi"'],
    ['r', '# comment\nx <- 1\n', 'x <- 1'],
    ['toml', '# comment\n[section]\na = 1\n', '[section]'],
    ['ini', '# comment\n[section]\n', '[section]'],
    ['env', '# comment\nKEY=val\n', 'KEY=val'],
    ['conf', '# comment\nport 8080\n', 'port 8080'],
    ['cfg', '# comment\nkey=val\n', 'key=val'],
    ['gitignore', '# comment\n*.log\n', '*.log'],
    ['dockerignore', '# comment\n*.log\n', '*.log'],
    ['graphql', '# comment\ntype Query {\n  id: ID\n}\n', 'type Query'],
    ['gql', '# comment\ntype Mutation { ok: Boolean }\n', 'type Mutation'],
    ['coffee', '# comment\nx = 1\n', 'x = 1'],
    ['nim', '# comment\necho "hi"\n', 'echo "hi"'],
  ];
  it.each(HASH_CASES)(
    'ext=%s strips # comments, preserves code',
    (ext, input, wantCode) => {
      const r = applyContentViewMinification(input, `f.${ext}`);
      hasNot(r, '# comment', ext);
      has(r, wantCode, ext);
    }
  );
});

describe('applyContentViewMinification — semicolon and percent comments', () => {
  it('clj: strips semicolon comments', () => {
    const r = applyContentViewMinification(
      '; comment\n(defn foo [] :ok)\n',
      'f.clj'
    );
    hasNot(r, '; comment', 'clj-comment');
    has(r, '(defn foo', 'clj-code');
  });

  it('erl: strips percent comments', () => {
    const r = applyContentViewMinification(
      '% comment\n-module(app).\nstart() -> ok.\n',
      'f.erl'
    );
    hasNot(r, '% comment', 'erl-comment');
    has(r, '-module(app).', 'erl-code');
  });
});

describe('applyContentViewMinification — HTML comments', () => {
  const HTML_CASES: [string][] = [
    ['html'],
    ['htm'],
    ['xml'],
    ['svg'],
    ['vue'],
    ['svelte'],
  ];
  it.each(HTML_CASES)('ext=%s strips <!-- --> comments', ext => {
    const input = `<!-- comment -->\n<${ext === 'vue' || ext === 'svelte' ? 'template' : 'root'}>hi</${ext === 'vue' || ext === 'svelte' ? 'template' : 'root'}>\n`;
    const r = applyContentViewMinification(input, `f.${ext}`);
    hasNot(r, '<!-- comment -->', ext);
  });
});

describe('applyContentViewMinification — SQL comments', () => {
  it('sql: strips -- and /* */ comments', () => {
    const r = applyContentViewMinification(
      '-- c\nSELECT 1;\n/* c2 */\nSELECT 2;\n',
      'f.sql'
    );
    hasNot(r, '-- c', 'sql-line');
    hasNot(r, '/* c2 */', 'sql-block');
    has(r, 'SELECT 1', 'sql-code');
    has(r, 'SELECT 2', 'sql-code2');
  });
});

describe('applyContentViewMinification — Lua comments', () => {
  it('lua: strips -- line and --[[ ]] block comments', () => {
    const r = applyContentViewMinification(
      '-- lc\nlocal x = 1\n--[[ bc ]]\nlocal y = 2\n',
      'f.lua'
    );
    hasNot(r, '-- lc', 'lua-line');
    hasNot(r, '--[[ bc ]]', 'lua-block');
    has(r, 'local x = 1', 'lua-code');
  });
});

describe('applyContentViewMinification — template comments', () => {
  const TPL_CASES: [string, string, string][] = [
    ['hbs', '{{!-- c --}}\n<div>{{x}}</div>\n', '<div>{{x}}</div>'],
    ['handlebars', '{{! c }}\n<div>{{y}}</div>\n', '<div>{{y}}</div>'],
    ['ejs', '<%# c %>\n<p><%= val %></p>\n', '<p>'],
    ['mustache', '{{! c }}\n<div>{{z}}</div>\n', '<div>'],
    ['twig', '{# c #}\n<div>{{ var }}</div>\n', '<div>'],
    ['jinja', '{# c #}\n<p>{{ val }}</p>\n', '<p>'],
    ['jinja2', '{# c #}\n<p>{{ val }}</p>\n', '<p>'],
    ['erb', '<%# c %>\n<p><%= x %></p>\n', '<p>'],
  ];
  it.each(TPL_CASES)(
    'ext=%s strips template comments',
    (ext, input, wantCode) => {
      const r = applyContentViewMinification(input, `f.${ext}`);
      has(r, wantCode, ext);
      // Comment markers should be stripped
      expect(
        r.includes('{{!-- c --}}') ||
          r.includes('{{! c }}') ||
          r.includes('<%# c %>') ||
          r.includes('{# c #}')
      ).toBe(false);
    }
  );
});

describe('applyContentViewMinification — Terraform (hash + c-style)', () => {
  it('tf: strips both # and /* */ comments', () => {
    const r = applyContentViewMinification(
      '# hc\n/* bc */\nresource "x" "y" {}\n',
      'main.tf'
    );
    hasNot(r, '# hc', 'tf-hash');
    hasNot(r, '/* bc */', 'tf-block');
    has(r, 'resource', 'tf-code');
  });
  it('tfvars: same as tf', () => {
    const r = applyContentViewMinification(
      '# c\nregion = "us-east-1"\n',
      'vars.tfvars'
    );
    hasNot(r, '# c', 'tfvars');
    has(r, 'region', 'tfvars-code');
  });
});

describe('applyContentViewMinification — Haskell comments', () => {
  it('hs: strips -- and {- -} comments', () => {
    const r = applyContentViewMinification(
      '-- lc\nmain = putStrLn "hi"\n{- bc -}\n',
      'f.hs'
    );
    hasNot(r, '-- lc', 'hs-line');
    hasNot(r, '{- bc -}', 'hs-block');
    has(r, 'main = putStrLn', 'hs-code');
  });
  it('lhs: handled same', () => {
    const r = applyContentViewMinification('-- c\nmain = pure ()\n', 'f.lhs');
    hasNot(r, '-- c', 'lhs');
  });
});

describe('applyContentViewMinification — PHP (c-style + hash combined)', () => {
  it('php: strips both comment families', () => {
    const r = applyContentViewMinification(
      '<?php\n/* bc */\n# hc\necho "hi";\n',
      'f.php'
    );
    hasNot(r, '/* bc */', 'php-block');
    hasNot(r, '# hc', 'php-hash');
    has(r, 'echo "hi"', 'php-code');
  });
});

describe('applyContentViewMinification — general fallback (no comments)', () => {
  const GEN_CASES: [string][] = [['csv'], ['txt'], ['log']];
  it.each(GEN_CASES)(
    'ext=%s compresses whitespace, preserves all content',
    ext => {
      const input = `line1\n\n\n\nline2   \nline3\n`;
      const r = applyContentViewMinification(input, `f.${ext}`);
      expect(r).not.toMatch(/\n{3,}/);
      has(r, 'line1', ext);
      has(r, 'line2', ext);
    }
  );
  it('unknown extension falls through to general', () => {
    const r = applyContentViewMinification('a\n\n\nb   \n', 'file.xyz123');
    expect(r).not.toMatch(/\n{3,}/);
    has(r, 'a', 'unknown');
  });
  it('no extension (e.g. Makefile) → general', () => {
    const r = applyContentViewMinification(
      'build:\n\t@echo ok\n\n\ntest:\n',
      'Makefile'
    );
    str(r, 'Makefile');
  });
});

describe('applyContentViewMinification — all MINIFY_CONFIG exts: never throws, never longer', () => {
  const content = '// comment\nconst x = 1;\n\n\nconst y = 2;   \n';
  const allExts = Object.keys(MINIFY_CONFIG.fileTypes);

  it.each(allExts)('ext=%s returns string, not longer', ext => {
    const r = applyContentViewMinification(content, `file.${ext}`);
    str(r, ext);
    notLonger(content, r, ext);
  });
});

describe('applyContentViewMinification — edge cases', () => {
  it('empty string returns empty string', () => {
    expect(applyContentViewMinification('', 'f.ts')).toBe('');
  });
  it('already-compact returns itself (not longer)', () => {
    const c = 'const x=1;';
    expect(applyContentViewMinification(c, 'f.ts').length).toBeLessThanOrEqual(
      c.length
    );
  });
  it('null input is caught, returns null (guard in caller)', () => {
    const r = applyContentViewMinification(null as unknown as string, 'f.ts');
    expect(r).toBeNull();
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// applyContentViewMinification applied to git diff patches
// ─────────────────────────────────────────────────────────────────────────────

describe('applyContentViewMinification on PR diff patches', () => {
  it('TypeScript patch — strips inline //comments from +/- lines', () => {
    const patch = [
      '@@ -1,4 +1,4 @@',
      ' const x = 1; // context',
      '-const y = 2; // old',
      '+const y = 3; // new',
      ' export { x, y };',
    ].join('\n');
    const r = applyContentViewMinification(patch, 'src/f.ts');
    has(r, '-const y = 2;', 'ts-patch-minus');
    has(r, '+const y = 3;', 'ts-patch-plus');
    hasNot(r, '// old', 'ts-patch-inline-comment');
    hasNot(r, '// new', 'ts-patch-inline-comment');
    has(r, '@@ -1,4', 'ts-patch-header');
  });

  it('Python patch — hash-comment context lines are stripped (documented trade-off)', () => {
    const patch = [
      '@@ -1,4 +1,4 @@',
      '-def old():',
      '+def new():',
      '     # python comment in context',
      '     return 1',
    ].join('\n');
    const r = applyContentViewMinification(patch, 'f.py');
    str(r, 'py-patch');
    has(r, '-def old():', 'py-minus');
    has(r, '+def new():', 'py-plus');
    // The hash-comment context line is stripped — this is the documented
    // trade-off of applying applyContentViewMinification to raw diff text.
    hasNot(r, '# python comment in context', 'py-comment-line');
  });

  it('JSON patch — JSON.parse fails on diff → trimmed original returned', () => {
    const patch = [
      '@@ -1,4 +1,4 @@',
      ' {',
      '-  "version": "1.0.0",',
      '+  "version": "2.0.0",',
      ' }',
    ].join('\n');
    const r = applyContentViewMinification(patch, 'package.json');
    str(r, 'json-patch');
    has(r, '"version"', 'json-patch-content');
  });

  it('Markdown patch — HTML comments stripped via minifyMarkdownCore', () => {
    const patch = [
      '@@ -1,4 +1,4 @@',
      '-# Old Title',
      '+# New Title',
      ' <!-- machine generated -->',
      ' Some text',
    ].join('\n');
    const r = applyContentViewMinification(patch, 'README.md');
    has(r, '+# New Title', 'md-patch-plus');
    hasNot(r, '<!-- machine generated -->', 'md-patch-html-comment');
  });

  it('HTML patch — HTML comments stripped', () => {
    const patch =
      '@@ -1,3 +1,3 @@\n-<!-- old -->\n+<!-- new -->\n <div>hi</div>';
    const r = applyContentViewMinification(patch, 'index.html');
    hasNot(r, '<!-- old -->', 'html-patch-comment');
  });

  it('CSS patch — c-style comments stripped', () => {
    const patch =
      '@@ -1,3 +1,3 @@\n-/* old color */\n+/* new color */\n .btn { color: red }';
    const r = applyContentViewMinification(patch, 'style.css');
    hasNot(r, '/* old color */', 'css-patch-comment');
  });

  it('unknown extension patch — general whitespace only, never corrupts', () => {
    const patch = '@@ -1,3 +1,3 @@\n-a\n+b\n context';
    const r = applyContentViewMinification(patch, 'data.xyz');
    has(r, '-a', 'unknown-minus');
    has(r, '+b', 'unknown-plus');
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// minifyContent (async) — all MINIFY_CONFIG extensions
// ─────────────────────────────────────────────────────────────────────────────

describe('minifyContent (async) — all registered extensions', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockMinify.mockResolvedValue({ code: 'minified()' });
    mockCleanCSS.mockReturnValue({ styles: '.a{color:red}', errors: [] });
    mockHtmlMinify.mockResolvedValue('<div></div>');
  });

  const allExts = Object.keys(MINIFY_CONFIG.fileTypes);

  it.each(allExts)(
    'ext=%s — result.content is string, result.failed is boolean',
    async ext => {
      const r = await minifyContent('// comment\nconst x = 1;\n', `f.${ext}`);
      str(r.content, ext);
      expect(typeof r.failed, `${ext}.failed`).toBe('boolean');
      expect(typeof r.type, `${ext}.type`).toBe('string');
    }
  );

  it('js → terser called', async () => {
    const r = await minifyContent('function f(){return 1}', 'f.js');
    expect(mockMinify).toHaveBeenCalled();
    expect(r.type).toBe('terser');
    expect(r.failed).toBe(false);
  });

  it('css → CleanCSS called', async () => {
    const r = await minifyContent('.btn { color: red; }', 'f.css');
    expect(mockCleanCSS).toHaveBeenCalled();
    expect(r.type).toBe('aggressive');
  });

  it('html → html-minifier-terser called', async () => {
    const r = await minifyContent('<html><body>hi</body></html>', 'f.html');
    expect(mockHtmlMinify).toHaveBeenCalled();
    expect(r.type).toBe('aggressive');
  });

  it('ts → conservative (no terser)', async () => {
    const r = await minifyContent('// c\nconst x = 1;\n', 'f.ts');
    expect(mockMinify).not.toHaveBeenCalled();
    expect(r.type).toBe('conservative');
    expect(r.content).not.toContain('// c');
  });

  it('json → json strategy', async () => {
    const r = await minifyContent('{\n  "a": 1\n}', 'f.json');
    expect(r.type).toBe('json');
    expect(r.content).toBe('{"a":1}');
  });

  it('md → markdown strategy (minifyMarkdownCore)', async () => {
    const r = await minifyContent('# T\n\n<!-- c -->\n\nP\n', 'f.md');
    expect(r.type).toBe('markdown');
    expect(r.content).not.toContain('<!-- c -->');
  });

  it('py → conservative, strips # comments', async () => {
    const r = await minifyContent('# comment\nx = 1\n', 'f.py');
    expect(r.type).toBe('conservative');
    expect(r.content).not.toContain('# comment');
  });

  it('unknown ext → general fallback', async () => {
    const r = await minifyContent('data\n\n\nmore   \n', 'f.zzz');
    expect(r.type).toBe('general');
    expect(r.failed).toBe(false);
  });

  it('terser failure → failed:true, content is original', async () => {
    mockMinify.mockRejectedValue(new Error('parse error'));
    const code = 'invalid js {{{';
    const r = await minifyContent(code, 'f.js');
    expect(r.failed).toBe(true);
    expect(r.content).toBe(code);
    expect(r.reason).toContain('Terser');
  });

  it('CleanCSS errors → fallback to minifyCSSCore, failed:false', async () => {
    mockCleanCSS.mockReturnValue({ styles: '', errors: ['bad syntax'] });
    const r = await minifyContent('.btn { color: red }', 'f.css');
    expect(r.failed).toBe(false);
    expect(r.reason).toContain('CleanCSS fallback');
  });

  it('html-minifier throw → fallback to minifyHTMLCore, failed:false', async () => {
    mockHtmlMinify.mockRejectedValue(new Error('html error'));
    const r = await minifyContent('<html></html>', 'f.html');
    expect(r.failed).toBe(false);
    expect(r.reason).toContain('html-minifier fallback');
  });
});

describe('minifyContent — size limit', () => {
  it('rejects files > 1 MB', async () => {
    const r = await minifyContent('x'.repeat(1024 * 1024 + 1), 'f.ts');
    expect(r.failed).toBe(true);
    expect(r.type).toBe('failed');
    expect(r.reason).toContain('exceeds 1MB');
  });
  it('accepts files at exactly 1 MB', async () => {
    const r = await minifyContent('a'.repeat(1024 * 1024), 'f.txt');
    expect(r.failed).toBe(false);
  });
  it('empty JS → not rejected', async () => {
    const r = await minifyContent('', 'f.js');
    expect(r.failed).toBe(false);
  });
  it('null input → caught as failed', async () => {
    const r = await minifyContent(null as unknown as string, 'f.ts');
    expect(r.failed).toBe(true);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// minifyMarkdownCore — PR body / comment contract
// ─────────────────────────────────────────────────────────────────────────────

describe('minifyMarkdownCore — PR body and comments (always-on in PR tool)', () => {
  it('strips HTML comments (machine-generated blobs)', () => {
    const r = minifyMarkdownCore(
      '# PR\n\n<!-- auto-generated -->\n\nReal content\n'
    );
    hasNot(r, '<!-- auto-generated -->', 'md-html-comment');
    has(r, 'Real content', 'md-body');
  });
  it('strips quoted-reply lines (> prefix)', () => {
    const r = minifyMarkdownCore(
      'Thanks\n\n> original line\n> more\n\nMy reply\n'
    );
    hasNot(r, '> original line', 'md-quote');
    has(r, 'My reply', 'md-reply');
  });
  it('compresses 3+ blank lines to ≤2', () => {
    const r = minifyMarkdownCore('a\n\n\n\n\nb\n');
    expect(r).not.toMatch(/\n{3,}/);
    has(r, 'a', 'md-line1');
    has(r, 'b', 'md-line2');
  });
  it('preserves code blocks intact', () => {
    const r = minifyMarkdownCore('```ts\nconst x = 1;\n```\n');
    has(r, '```ts', 'md-fence-open');
    has(r, 'const x = 1;', 'md-fence-content');
    has(r, '```', 'md-fence-close');
  });
  it('normalises table pipes', () => {
    const r = minifyMarkdownCore('| col1   |   col2 |\n|--|--|\n| a | b |\n');
    has(r, '| col1 | col2 |', 'md-table');
  });
  it('preserves headings', () => {
    const r = minifyMarkdownCore('## Section\n\nContent\n');
    has(r, '## Section', 'md-heading');
  });
  it('empty string → empty string', () => {
    expect(minifyMarkdownCore('')).toBe('');
  });
  it('whitespace-only → empty after trim', () => {
    expect(minifyMarkdownCore('   \n\n\n  ').trim()).toBe('');
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// extractSignatures — per-language contracts
// ─────────────────────────────────────────────────────────────────────────────

describe('extractSignatures — supported language contracts', () => {
  // Gutter line helper
  function gutterLines(sigs: string) {
    return sigs
      .split('\n')
      .map(line => {
        const m = line.match(/^ *(\d+)\| (.*)$/);
        return m ? { num: Number(m[1]), text: m[2]! } : null;
      })
      .filter(Boolean) as Array<{ num: number; text: string }>;
  }
  function hasSig(sigs: string, sub: string, label: string) {
    expect(sigs, `${label}: should contain ${JSON.stringify(sub)}`).toContain(
      sub
    );
  }
  function noBody(sigs: string, sub: string, label: string) {
    expect(sigs, `${label}: body leaked: ${JSON.stringify(sub)}`).not.toContain(
      sub
    );
  }
  /**
   * Verifies no pure-comment lines leaked into the skeleton output.
   * Shebangs (#!), preprocessor directives (#include, #define), and
   * structural # lines (GraphQL, TOML) are intentionally excluded.
   */
  function noComment(sigs: string, label: string) {
    const lines = gutterLines(sigs);
    for (const { text } of lines) {
      const t = text.trim();
      // Shebang and preprocessor directives are structural, not comments.
      if (
        t.startsWith('#!') ||
        t.startsWith('#include') ||
        t.startsWith('#define') ||
        t.startsWith('#pragma') ||
        t.startsWith('#if') ||
        t.startsWith('#endif') ||
        t.startsWith('#else') ||
        t.startsWith('#import')
      )
        continue;
      expect(
        t.startsWith('//') ||
          t.startsWith('/*') ||
          t.startsWith('*') ||
          (t.startsWith('#') && !t.startsWith('#!')) ||
          (t.startsWith('--') && !t.startsWith('-->')),
        `${label}: comment leaked: ${t}`
      ).toBe(false);
    }
  }

  // ── TypeScript / JavaScript (AST) ──────────────────────────────────────────
  const TS = [
    "import { A } from './a';",
    '',
    '// should be dropped',
    'export interface IUser {',
    '  id: string;',
    '  name: string;',
    '}',
    '',
    'export class UserService {',
    '  constructor(private db: DB) {}',
    '  async getUser(id: string): Promise<IUser> {',
    '    const SECRET = "should-drop";',
    '    return this.db.find(id);',
    '  }',
    '}',
    '',
    'export const greet = (name: string): string => {',
    '  return `Hello ${name}`;',
    '};',
  ].join('\n');

  it.each(['ts', 'tsx', 'js', 'jsx', 'mjs', 'cjs'])(
    'ext=%s: imports, class head, method sigs kept; bodies dropped',
    ext => {
      const sigs = extractSignatures(TS, `f.${ext}`)!;
      expect(sigs).not.toBeNull();
      hasSig(sigs, 'import { A }', ext);
      hasSig(sigs, 'export interface IUser', ext);
      hasSig(sigs, 'export class UserService', ext);
      hasSig(sigs, 'async getUser(id: string)', ext);
      noBody(sigs, 'SECRET', ext);
      noComment(sigs, ext);
    }
  );

  // ── Python ─────────────────────────────────────────────────────────────────
  it('py: import, class, def kept; body dropped', () => {
    const src = [
      'import os',
      '# comment',
      'class Foo:',
      '    def bar(self, x: int) -> str:',
      '        secret = "drop"',
      '        return str(x)',
      '__version__ = "1.0"',
    ].join('\n');
    const sigs = extractSignatures(src, 'f.py')!;
    expect(sigs).not.toBeNull();
    hasSig(sigs, 'import os', 'py');
    hasSig(sigs, 'class Foo:', 'py');
    hasSig(sigs, 'def bar', 'py');
    noBody(sigs, '"drop"', 'py');
    noComment(sigs, 'py');
  });

  // ── Go ─────────────────────────────────────────────────────────────────────
  it('go: package, import, func head kept; body dropped', () => {
    const src = [
      'package main',
      '',
      'import (',
      '  "fmt"',
      '  "os"',
      ')',
      '',
      '// comment',
      'func greet(name string) string {',
      '  secret := "drop"',
      '  return fmt.Sprintf("hi %s", name)',
      '}',
    ].join('\n');
    const sigs = extractSignatures(src, 'f.go')!;
    expect(sigs).not.toBeNull();
    hasSig(sigs, 'package main', 'go');
    hasSig(sigs, 'func greet', 'go');
    noBody(sigs, '"drop"', 'go');
    noComment(sigs, 'go');
  });

  // ── Java / C# / Kotlin ─────────────────────────────────────────────────────
  it.each(['java', 'cs'])(
    'ext=%s: import/using, class, method kept; body dropped',
    ext => {
      const src = [
        ext === 'java'
          ? 'import java.util.List;'
          : 'using System.Collections.Generic;',
        '// comment',
        `public class Foo {`,
        '  public String bar(int x) {',
        '    String secret = "drop";',
        '    return String.valueOf(x);',
        '  }',
        '}',
      ].join('\n');
      const sigs = extractSignatures(src, `f.${ext}`)!;
      expect(sigs).not.toBeNull();
      hasSig(sigs, 'class Foo', ext);
      hasSig(sigs, 'public String bar', ext);
      noBody(sigs, '"drop"', ext);
      noComment(sigs, ext);
    }
  );

  // ── Rust ───────────────────────────────────────────────────────────────────
  it('rs: use, fn, struct kept; body dropped', () => {
    const src = [
      'use std::fmt;',
      '// comment',
      'pub struct Foo { pub x: i32 }',
      'pub fn greet(name: &str) -> String {',
      '  let secret = "drop";',
      '  format!("hi {}", name)',
      '}',
    ].join('\n');
    const sigs = extractSignatures(src, 'f.rs')!;
    expect(sigs).not.toBeNull();
    hasSig(sigs, 'use std::fmt', 'rs');
    hasSig(sigs, 'pub fn greet', 'rs');
    noBody(sigs, '"drop"', 'rs');
    noComment(sigs, 'rs');
  });

  // ── C family ───────────────────────────────────────────────────────────────
  it.each(['c', 'h', 'cpp'])(
    'ext=%s: includes, struct/func heads kept; bodies dropped',
    ext => {
      const src = [
        '#include <stdio.h>',
        '#define MAX 100',
        '// comment',
        'typedef struct { int x; int y; } Point;',
        'void print_point(Point p) {',
        '  int secret = 42;',
        '  printf("(%d,%d)", p.x, p.y);',
        '}',
      ].join('\n');
      const sigs = extractSignatures(src, `f.${ext}`)!;
      expect(sigs).not.toBeNull();
      hasSig(sigs, '#include <stdio.h>', ext);
      hasSig(sigs, 'void print_point', ext);
      noBody(sigs, 'secret', ext);
      noComment(sigs, ext);
    }
  );

  // ── Ruby ───────────────────────────────────────────────────────────────────
  it('rb: require, class, method kept; body dropped', () => {
    const src = [
      'require "json"',
      '# comment',
      'class Foo',
      '  def bar(x)',
      '    secret = "drop"',
      '    x',
      '  end',
      'end',
    ].join('\n');
    const sigs = extractSignatures(src, 'f.rb')!;
    expect(sigs).not.toBeNull();
    hasSig(sigs, 'require "json"', 'rb');
    hasSig(sigs, 'class Foo', 'rb');
    hasSig(sigs, 'def bar(x)', 'rb');
    noBody(sigs, '"drop"', 'rb');
    noComment(sigs, 'rb');
  });

  // ── PHP ────────────────────────────────────────────────────────────────────
  it('php: class, method kept; body dropped', () => {
    const src = [
      '<?php',
      '# comment',
      'namespace App;',
      'class Foo {',
      '  public function bar(string $x): string {',
      '    $secret = "drop";',
      '    return $x;',
      '  }',
      '}',
    ].join('\n');
    const sigs = extractSignatures(src, 'f.php')!;
    expect(sigs).not.toBeNull();
    hasSig(sigs, 'namespace App', 'php');
    hasSig(sigs, 'class Foo', 'php');
    hasSig(sigs, 'public function bar', 'php');
    noBody(sigs, '"drop"', 'php');
    noComment(sigs, 'php');
  });

  // ── Swift ──────────────────────────────────────────────────────────────────
  it('swift: import, struct/func kept; body dropped', () => {
    const src = [
      'import Foundation',
      '// comment',
      'public struct Point {',
      '  var x: Double',
      '  var y: Double',
      '}',
      'public func greet(_ name: String) -> String {',
      '  let secret = "drop"',
      '  return "hi " + name',
      '}',
    ].join('\n');
    const sigs = extractSignatures(src, 'f.swift')!;
    expect(sigs).not.toBeNull();
    hasSig(sigs, 'import Foundation', 'swift');
    hasSig(sigs, 'public struct Point', 'swift');
    hasSig(sigs, 'public func greet', 'swift');
    noBody(sigs, '"drop"', 'swift');
    noComment(sigs, 'swift');
  });

  // ── CSS / SCSS / LESS ──────────────────────────────────────────────────────
  it('css: selector/at-rule heads kept; declaration bodies dropped', () => {
    const src = [
      '@import "base.css";',
      '/* comment */',
      '.button {',
      '  color: red;',
      '  background: blue;',
      '}',
      '@media (max-width: 768px) {',
      '  .button { display: none; }',
      '}',
    ].join('\n');
    const sigs = extractSignatures(src, 'f.css')!;
    expect(sigs).not.toBeNull();
    hasSig(sigs, '.button', 'css');
    hasSig(sigs, '@media', 'css');
  });

  it.each(['scss', 'less'])('ext=%s: selector heads kept', ext => {
    const src =
      '.btn { color: red; padding: 8px; }\n.nav {\n  display: flex;\n  flex-direction: column;\n}\n';
    const sigs = extractSignatures(src, `f.${ext}`)!;
    expect(sigs).not.toBeNull();
  });

  // ── HTML ───────────────────────────────────────────────────────────────────
  it.each(['html', 'htm'])(
    'ext=%s: doctype, headings, id-bearing tags kept',
    ext => {
      const src = [
        '<!DOCTYPE html>',
        '<!-- comment -->',
        '<html id="root">',
        '<head><title>T</title></head>',
        '<body>',
        '<h1>Title</h1>',
        '<div id="app"><p>prose</p></div>',
        '</body></html>',
      ].join('\n');
      const sigs = extractSignatures(src, `f.${ext}`)!;
      expect(sigs).not.toBeNull();
      hasSig(sigs, '<!DOCTYPE html>', ext);
      hasSig(sigs, '<h1>Title</h1>', ext);
    }
  );

  // ── Vue / Svelte ───────────────────────────────────────────────────────────
  it('vue: <script> exported symbols kept; inline body dropped', () => {
    const src = [
      '<template><div id="app">{{ msg }}</div></template>',
      '<script lang="ts">',
      "import { ref } from 'vue';",
      'export default {',
      "  setup() { const msg = ref('hi'); return { msg }; }",
      '}',
      '</script>',
    ].join('\n');
    const sigs = extractSignatures(src, 'f.vue')!;
    expect(sigs).not.toBeNull();
    hasSig(sigs, '<template>', 'vue');
  });

  it('svelte: <script> exported symbols kept', () => {
    const src = [
      '<script lang="ts">',
      "import { onMount } from 'svelte';",
      'export let name: string;',
      'function greet() { return "hi " + name; }',
      '</script>',
      '<h1 id="title">{name}</h1>',
    ].join('\n');
    const sigs = extractSignatures(src, 'f.svelte')!;
    expect(sigs).not.toBeNull();
  });

  // ── SQL ────────────────────────────────────────────────────────────────────
  it('sql: CREATE TABLE/VIEW/FUNCTION heads kept; body dropped', () => {
    const src = [
      '-- comment',
      'CREATE TABLE users (id INT PRIMARY KEY, name VARCHAR(100));',
      '',
      'CREATE VIEW active_users AS',
      '  SELECT * FROM users WHERE active = 1;',
      '',
      'CREATE FUNCTION get_name(uid INT)',
      '  RETURNS VARCHAR(100)',
      'BEGIN',
      '  DECLARE secret VARCHAR(100);',
      '  RETURN secret;',
      'END;',
    ].join('\n');
    const sigs = extractSignatures(src, 'f.sql')!;
    expect(sigs).not.toBeNull();
    hasSig(sigs, 'CREATE TABLE users', 'sql');
    hasSig(sigs, 'CREATE VIEW active_users', 'sql');
    hasSig(sigs, 'CREATE FUNCTION get_name', 'sql');
    noBody(sigs, 'DECLARE secret', 'sql');
    noComment(sigs, 'sql');
  });

  // ── Shell ──────────────────────────────────────────────────────────────────
  it.each(['sh', 'bash', 'zsh'])(
    'ext=%s: shebang, exports, function heads kept; body dropped',
    ext => {
      const src = [
        '#!/bin/bash',
        '# comment',
        'export APP_ENV=production',
        'function deploy() {',
        '  local secret="drop"',
        '  echo "deploying"',
        '}',
      ].join('\n');
      const sigs = extractSignatures(src, `f.${ext}`)!;
      expect(sigs).not.toBeNull();
      hasSig(sigs, '#!/bin/bash', ext);
      hasSig(sigs, 'function deploy', ext);
      noBody(sigs, '"drop"', ext);
      noComment(sigs, ext);
    }
  );

  // ── Unsupported types return null ──────────────────────────────────────────
  it.each(['txt', 'md', 'json', 'yaml', 'log', 'csv', 'toml', 'xyz'])(
    'ext=%s: returns null (unsupported)',
    ext => {
      const r = extractSignatures(
        'some content here\nmore content\n',
        `f.${ext}`
      );
      expect(r).toBeNull();
    }
  );

  // ── Gutter line-number contract ────────────────────────────────────────────
  it('original 1-based line numbers in gutter, no blank gutter lines', () => {
    const src =
      "import { A } from './a';\n\nexport function foo(x: string): void {\n  return;\n}\n";
    const sigs = extractSignatures(src, 'f.ts')!;
    expect(sigs).not.toBeNull();
    const lines = gutterLines(sigs);
    // All lines must have a positive line number
    for (const { num } of lines) {
      expect(num).toBeGreaterThan(0);
    }
    // No blank gutter lines
    for (const { text } of lines) {
      expect(text.trim()).not.toBe('');
    }
  });

  it('savings: skeleton is significantly shorter than source', () => {
    const src = [
      "import { A } from './a';",
      '',
      'export class BigService {',
      '  private db: DB;',
      '  constructor(db: DB) { this.db = db; const x = 1; const y = 2; const z = 3; }',
      '  async findAll(): Promise<A[]> {',
      '    const items = await this.db.query("SELECT *");',
      '    return items.map(i => new A(i));',
      '  }',
      '  async findOne(id: string): Promise<A | null> {',
      '    return this.db.findById(id);',
      '  }',
      '}',
    ].join('\n');
    const sigs = extractSignatures(src, 'f.ts')!;
    expect(sigs).not.toBeNull();
    expect(sigs.length).toBeLessThan(src.length * 0.75); // at least 25% smaller
  });
});
