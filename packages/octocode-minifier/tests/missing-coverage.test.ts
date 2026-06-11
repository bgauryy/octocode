/**
 * Missing-coverage tests — fills gaps identified after auditing all existing
 * test files.
 *
 * Gaps addressed:
 *
 *  1. extractSignatures — registered language extensions with NO dedicated tests:
 *       kt   → javaCsStrategy
 *       hpp  → cFamilyStrategy
 *       cc   → cFamilyStrategy
 *
 *  2. Exported strategy functions with no direct unit tests:
 *       minifyJsonReadable   (tested only via applyContentViewMinification)
 *       minifyJavaScriptCore (tested only via minifyContentSync)
 *       minifyConservativeCore (tested only via minifyContent async path)
 *
 *  3. Exported constant never asserted:
 *       SIGNATURES_ONLY_HINT
 *
 *  4. Long-name skeleton aliases:
 *       kotlin / rust
 */

import { describe, it, expect } from 'vitest';
import {
  extractSignatures,
  SIGNATURES_ONLY_HINT,
  SUPPORTED_SIGNATURE_EXTENSIONS,
  minifyJsonReadable,
  minifyJavaScriptCore,
  minifyConservativeCore,
  minifyGeneralCore,
} from '@octocodeai/octocode-minifier';
import type { FileTypeMinifyConfig } from '@octocodeai/octocode-minifier';

// ─── helpers ──────────────────────────────────────────────────────────────────

interface GutterLine {
  num: number;
  text: string;
}

function gutterLines(sigs: string): GutterLine[] {
  return sigs
    .split('\n')
    .map(line => {
      const m = line.match(/^ *(\d+)\| (.*)$/);
      return m ? { num: Number(m[1]), text: m[2]! } : null;
    })
    .filter(Boolean) as GutterLine[];
}

function expectGutter(sigs: string, num: number, substring: string): void {
  const entry = gutterLines(sigs).find(l => l.num === num);
  expect(entry, `expected gutter entry for line ${num}`).toBeDefined();
  expect(entry!.text).toContain(substring);
}

function expectNoBlankGutterLines(sigs: string): void {
  for (const line of sigs.split('\n')) {
    expect(line).toMatch(/^ *\d+\| .*\S/);
  }
}

/** No pure c-style or hash comment lines in the skeleton output. */
function expectNoCommentLines(sigs: string): void {
  for (const { text } of gutterLines(sigs)) {
    const t = text.trim();
    // Preprocessor directives (#include, #define, #pragma) are structural.
    if (
      t.startsWith('#include') ||
      t.startsWith('#define') ||
      t.startsWith('#pragma')
    )
      continue;
    expect(t.startsWith('//'), `comment leaked: ${t}`).toBe(false);
    expect(t.startsWith('/*'), `comment leaked: ${t}`).toBe(false);
    expect(t.startsWith('*'), `comment leaked: ${t}`).toBe(false);
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. extractSignatures — kt (Kotlin)
// ─────────────────────────────────────────────────────────────────────────────

// javaCsStrategy patterns match:
//   • import / using / package / namespace
//   • class / interface / enum / record + name
//   • public | private | protected | static | abstract | final | override prefix

const KT_SOURCE = `import kotlinx.coroutines.runBlocking

// comment should vanish
interface Repository {
    fun findAll(): List<String>
    fun findById(id: Int): String?
}

class UserService : Repository {
    private val cache: MutableMap<Int, String> = mutableMapOf()

    override fun findAll(): List<String> {
        val secret = "drop"
        return cache.values.toList()
    }

    override fun findById(id: Int): String? {
        val secret2 = "drop"
        return cache[id]
    }

    protected fun clear() {
        val secretBody = "drop"
        cache.clear()
    }
}

abstract class BaseService {
    abstract fun init(): Unit
}

enum class Status {
    ACTIVE,
    INACTIVE,
}
`;

describe('extractSignatures — kt (Kotlin / javaCsStrategy)', () => {
  const sigs = extractSignatures(KT_SOURCE, 'UserService.kt')!;

  it('returns a non-null skeleton', () => {
    expect(sigs).not.toBeNull();
  });

  it('keeps import statements', () => {
    expectGutter(sigs, 1, 'import kotlinx.coroutines.runBlocking');
  });

  it('keeps interface and class declarations', () => {
    expect(sigs).toContain('interface Repository');
    expect(sigs).toContain('class UserService');
    expect(sigs).toContain('abstract class BaseService');
    expect(sigs).toContain('enum class Status');
  });

  it('keeps private/override/protected/abstract-prefixed members', () => {
    expect(sigs).toContain('private val cache');
    expect(sigs).toContain('override fun findAll');
    expect(sigs).toContain('override fun findById');
    expect(sigs).toContain('protected fun clear');
    expect(sigs).toContain('abstract fun init');
  });

  it('drops function bodies', () => {
    expect(sigs).not.toContain('"drop"');
    expect(sigs).not.toContain('secretBody');
    expect(sigs).not.toContain('secret2');
    expect(sigs).not.toContain('cache.clear()');
  });

  it('drops comments', () => {
    expect(sigs).not.toContain('comment should vanish');
    expectNoCommentLines(sigs);
  });

  it('emits no blank gutter lines', () => {
    expectNoBlankGutterLines(sigs);
  });

  it('skeleton is shorter than the source', () => {
    expect(sigs.length).toBeLessThan(KT_SOURCE.length);
  });

  it('supports the long-name .kotlin extension alias', () => {
    const aliasSigs = extractSignatures(KT_SOURCE, 'UserService.kotlin')!;

    expect(aliasSigs).not.toBeNull();
    expect(aliasSigs).toContain('interface Repository');
    expect(aliasSigs).toContain('class UserService');
    expect(aliasSigs).not.toContain('"drop"');
  });
});

describe('signature extension aliases', () => {
  it('exports long-name aliases next to short extensions', () => {
    expect(SUPPORTED_SIGNATURE_EXTENSIONS).toEqual(
      expect.arrayContaining(['kt', 'kotlin', 'rs', 'rust'])
    );
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 2. extractSignatures — hpp (C++ header / cFamilyStrategy)
// ─────────────────────────────────────────────────────────────────────────────

const HPP_SOURCE = `#pragma once
#include <string>
#include <vector>

// comment should vanish

namespace net {

/* block comment should vanish */
class Connection {
public:
    explicit Connection(int fd, const std::string& host);
    ~Connection();

    bool send(const std::string& payload);
    bool recv(std::string& out);

private:
    int fd_;
    std::string host_;
};

template <typename T>
struct Result {
    T value;
    bool ok;
};

typedef unsigned int uint32;

} // namespace net
`;

describe('extractSignatures — hpp (C++ header / cFamilyStrategy)', () => {
  const sigs = extractSignatures(HPP_SOURCE, 'connection.hpp')!;

  it('returns a non-null skeleton', () => {
    expect(sigs).not.toBeNull();
  });

  it('keeps #include directives (cFamilyStrategy matches #include/#define, not #pragma)', () => {
    // C_PREPROC = /^\s*#\s*(?:include|define)\b/ — #pragma is NOT matched
    expect(sigs).toContain('#include <string>');
    expect(sigs).toContain('#include <vector>');
    expect(sigs).not.toContain('#pragma once'); // correctly not kept by the strategy
  });

  it('keeps class declaration with member signatures', () => {
    expect(sigs).toContain('class Connection {');
    expect(sigs).toContain(
      'explicit Connection(int fd, const std::string& host);'
    );
    expect(sigs).toContain('bool send(const std::string& payload);');
    expect(sigs).toContain('bool recv(std::string& out);');
    expect(sigs).toContain('int fd_;');
    expect(sigs).toContain('std::string host_;');
  });

  it('keeps struct declaration with its fields', () => {
    expect(sigs).toContain('struct Result {');
    expect(sigs).toContain('T value;');
    expect(sigs).toContain('bool ok;');
  });

  it('plain typedef (no struct/union/enum/class) is NOT kept by cFamilyStrategy', () => {
    // C_TYPE_BLOCK = /^(?:typedef\s+)?(?:struct|union|enum|class)\b/ only matches structural
    // typedef declarations — a plain type alias like `typedef unsigned int uint32;` is not kept.
    expect(sigs).not.toContain('typedef unsigned int uint32');
  });

  it('drops all comments', () => {
    expect(sigs).not.toContain('comment should vanish');
    expect(sigs).not.toContain('block comment should vanish');
    expectNoCommentLines(sigs);
  });

  it('emits no blank gutter lines', () => {
    expectNoBlankGutterLines(sigs);
  });

  it('skeleton is shorter than the source', () => {
    expect(sigs.length).toBeLessThan(HPP_SOURCE.length);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 3. extractSignatures — cc (C++ source variant / cFamilyStrategy)
// ─────────────────────────────────────────────────────────────────────────────

const CC_SOURCE = `#include "connection.hpp"
#include <cerrno>

// comment should vanish
namespace net {

/* block comment */
Connection::Connection(int fd, const std::string& host)
    : fd_(fd), host_(host) {
    int secretInit = 0;
}

Connection::~Connection() {
    int secretDtor = 0;
    close(fd_);
}

bool Connection::send(const std::string& payload) {
    const int secretLen = payload.size();
    return write(fd_, payload.data(), secretLen) >= 0;
}

static bool validate_fd(int fd) {
    const int secretCheck = fd;
    return fd >= 0;
}

} // namespace net
`;

describe('extractSignatures — cc (C++ source variant / cFamilyStrategy)', () => {
  const sigs = extractSignatures(CC_SOURCE, 'connection.cc')!;

  it('returns a non-null skeleton', () => {
    expect(sigs).not.toBeNull();
  });

  it('keeps #include directives', () => {
    expect(sigs).toContain('#include "connection.hpp"');
    expect(sigs).toContain('#include <cerrno>');
  });

  it('keeps constructor, destructor, and method definition heads', () => {
    expectGutter(
      sigs,
      8,
      'Connection::Connection(int fd, const std::string& host)'
    );
    expectGutter(sigs, 13, 'Connection::~Connection() {');
    expectGutter(
      sigs,
      18,
      'bool Connection::send(const std::string& payload) {'
    );
    expectGutter(sigs, 23, 'static bool validate_fd(int fd) {');
  });

  it('drops all function bodies', () => {
    expect(sigs).not.toContain('secretInit');
    expect(sigs).not.toContain('secretDtor');
    expect(sigs).not.toContain('secretLen');
    expect(sigs).not.toContain('secretCheck');
    expect(sigs).not.toContain('close(fd_)');
    expect(sigs).not.toContain('write(fd_');
  });

  it('drops all comments', () => {
    expect(sigs).not.toContain('comment should vanish');
    expect(sigs).not.toContain('block comment');
    expectNoCommentLines(sigs);
  });

  it('emits no blank gutter lines', () => {
    expectNoBlankGutterLines(sigs);
  });

  it('skeleton is shorter than the source', () => {
    expect(sigs.length).toBeLessThan(CC_SOURCE.length);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 4. minifyJsonReadable — direct unit tests
// ─────────────────────────────────────────────────────────────────────────────

describe('minifyJsonReadable — direct unit tests', () => {
  it('already-valid JSON is returned unchanged (failed:false, content unchanged)', () => {
    const input = '{\n  "a": 1,\n  "b": 2\n}';
    const result = minifyJsonReadable(input);
    expect(result.failed).toBe(false);
    expect(result.content).toBe(input);
  });

  it('already-compact valid JSON is returned unchanged', () => {
    const input = '{"a":1,"b":2}';
    const result = minifyJsonReadable(input);
    expect(result.failed).toBe(false);
    expect(result.content).toBe(input);
  });

  it('JSONC with // line comments — strips them, preserves structure, returns valid JSON', () => {
    const input =
      '{\n  // package identity\n  "name": "demo",\n  "version": "1.0.0"\n}';
    const result = minifyJsonReadable(input);
    expect(result.failed).toBe(false);
    expect(result.content).not.toContain('// package identity');
    expect(result.content).toContain('"name": "demo"');
    expect(result.content).toContain('"version": "1.0.0"');
    // Result must be valid JSON
    expect(() => JSON.parse(result.content)).not.toThrow();
  });

  it('JSONC with /* */ block comments — strips them', () => {
    const input = '{\n  /* ignore */\n  "a": 1\n}';
    const result = minifyJsonReadable(input);
    expect(result.failed).toBe(false);
    expect(result.content).not.toContain('/* ignore */');
    expect(result.content).toContain('"a": 1');
    expect(() => JSON.parse(result.content)).not.toThrow();
  });

  it('JSON5 with trailing commas — strips them, preserves structure', () => {
    const input = '{\n  "items": [\n    "a",\n    "b",\n  ],\n  "ok": true,\n}';
    const result = minifyJsonReadable(input);
    expect(result.failed).toBe(false);
    expect(result.content).toContain('"items"');
    expect(result.content).toContain('"a"');
    expect(() => JSON.parse(result.content)).not.toThrow();
  });

  it('// inside a string value is NOT treated as a comment', () => {
    const input = '{\n  "url": "https://example.com/a // literal"\n}';
    const result = minifyJsonReadable(input);
    expect(result.failed).toBe(false);
    expect(result.content).toContain('https://example.com/a // literal');
  });

  it('unparseable input (invalid JSON even after cleaning) — returns trimmed original, failed:false', () => {
    const input = '{ completely broken: json ]]]';
    const result = minifyJsonReadable(input);
    expect(result.failed).toBe(false);
    expect(result.content).toBe(input.trim());
  });

  it('preserves multi-line pretty-print structure (does NOT collapse to single line)', () => {
    const input = '{\n  // comment\n  "a": 1,\n  "b": 2\n}';
    const result = minifyJsonReadable(input);
    // Must still contain newlines — readable mode, not minified
    expect(result.content).toContain('\n');
  });

  it('returns object with content string and failed boolean', () => {
    const result = minifyJsonReadable('{"x":1}');
    expect(typeof result.content).toBe('string');
    expect(typeof result.failed).toBe('boolean');
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 5. minifyJavaScriptCore — direct unit tests
// ─────────────────────────────────────────────────────────────────────────────

describe('minifyJavaScriptCore — direct unit tests', () => {
  it('strips // line comments', () => {
    const result = minifyJavaScriptCore('// top comment\nconst x = 1;');
    expect(result).not.toContain('// top comment');
    expect(result).toContain('const x = 1;');
  });

  it('strips /* */ block comments', () => {
    const result = minifyJavaScriptCore('/* block */\nconst x = 1;');
    expect(result).not.toContain('/* block */');
    expect(result).toContain('const x');
  });

  it('strips inline // comments (space before)', () => {
    const result = minifyJavaScriptCore('const x = 1; // inline');
    expect(result).not.toContain('// inline');
  });

  it('collapses whitespace — consecutive spaces reduced to one', () => {
    const result = minifyJavaScriptCore('const   x   =   1;');
    expect(result).not.toContain('   ');
  });

  it('compacts brace/paren/semicolon spacing', () => {
    const result = minifyJavaScriptCore('function f ( ) { return 1 ; }');
    expect(result).not.toContain('f ( )');
    expect(result).toContain('f()');
  });

  it('removes blank lines — output has no empty lines', () => {
    const result = minifyJavaScriptCore('const a = 1;\n\n\nconst b = 2;');
    const lines = result.split('\n').filter(l => l.trim() === '');
    expect(lines).toHaveLength(0);
  });

  it('returns a string shorter than the input', () => {
    const input =
      '// comment\nfunction greet(   name   ) {\n  return "Hello " + name;\n}\n';
    expect(minifyJavaScriptCore(input).length).toBeLessThan(input.length);
  });

  it('handles empty string without throwing', () => {
    expect(minifyJavaScriptCore('')).toBe('');
  });

  it('handles whitespace-only content', () => {
    const result = minifyJavaScriptCore('   \n\n\t  ');
    // All-whitespace becomes empty after trimming empty lines
    expect(result.trim()).toBe('');
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 6. minifyConservativeCore — direct unit tests
// ─────────────────────────────────────────────────────────────────────────────

describe('minifyConservativeCore — direct unit tests', () => {
  const cConfig: FileTypeMinifyConfig = {
    strategy: 'conservative',
    comments: 'c-style',
  };
  const hashConfig: FileTypeMinifyConfig = {
    strategy: 'conservative',
    comments: 'hash',
  };
  const noCommentConfig: FileTypeMinifyConfig = { strategy: 'conservative' };
  const multiConfig: FileTypeMinifyConfig = {
    strategy: 'conservative',
    comments: ['c-style', 'hash'],
  };

  it('strips c-style line comments', () => {
    const result = minifyConservativeCore(
      '// comment\nconst x = 1;\n',
      cConfig
    );
    expect(result).not.toContain('// comment');
    expect(result).toContain('const x = 1;');
  });

  it('strips c-style block comments', () => {
    const result = minifyConservativeCore(
      '/* block */\nconst x = 1;\n',
      cConfig
    );
    expect(result).not.toContain('/* block */');
    expect(result).toContain('const x = 1;');
  });

  it('strips hash comments', () => {
    const result = minifyConservativeCore('# comment\nx = 1\n', hashConfig);
    expect(result).not.toContain('# comment');
    expect(result).toContain('x = 1');
  });

  it('strips multiple comment families when passed as array', () => {
    const result = minifyConservativeCore(
      '/* block */\n# hash\ncode = 1\n',
      multiConfig
    );
    expect(result).not.toContain('/* block */');
    expect(result).not.toContain('# hash');
    expect(result).toContain('code = 1');
  });

  it('preserves non-first-line indentation (.trim() only affects first/last line)', () => {
    // .trim() in minifyConservativeCore removes leading whitespace from the first line
    // of the WHOLE result, but inner-line indentation is preserved.
    const input = 'def foo():\n    return 1\n    # body\n';
    const result = minifyConservativeCore(input, hashConfig);
    // def foo(): is at column 0 — still there
    expect(result).toContain('def foo():');
    // inner indented line is preserved
    expect(result).toContain('    return 1');
  });

  it('removes trailing whitespace from each line', () => {
    const result = minifyConservativeCore(
      'const x = 1;   \nconst y = 2;\t\n',
      cConfig
    );
    expect(result).not.toMatch(/[ \t]+$/m);
  });

  it('collapses 3+ consecutive blank lines to at most 2', () => {
    const result = minifyConservativeCore('a\n\n\n\n\nb\n', cConfig);
    expect(result).not.toMatch(/\n{3,}/);
    expect(result).toContain('a');
    expect(result).toContain('b');
  });

  it('normalizes CRLF input before blank-line compression', () => {
    const input = 'const x = 1;\r\n\r\n\r\nconst y = 2;\r\n';
    const result = minifyConservativeCore(input, cConfig);

    expect(result).toBe('const x = 1;\n\nconst y = 2;');
    expect(result).not.toContain('\r\n');
  });

  it('trims the result', () => {
    const result = minifyConservativeCore('\n\n\nconst x = 1;\n\n\n', cConfig);
    expect(result).not.toMatch(/^\s/);
    expect(result).not.toMatch(/\s$/);
  });

  it('no comments config — only whitespace normalisation applied', () => {
    const input = '# this is kept (no comments config)\ncode()\n\n\n\n';
    const result = minifyConservativeCore(input, noCommentConfig);
    expect(result).toContain('# this is kept');
    expect(result).not.toMatch(/\n{3,}/);
  });

  it('handles empty string', () => {
    expect(minifyConservativeCore('', cConfig)).toBe('');
  });

  it('result is shorter than or equal to input when comments are present', () => {
    const input = '// long comment that adds length\nconst x = 1;\n\n\n\n';
    const result = minifyConservativeCore(input, cConfig);
    expect(result.length).toBeLessThanOrEqual(input.length);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 7. minifyGeneralCore — direct unit tests
// ─────────────────────────────────────────────────────────────────────────────

describe('minifyGeneralCore — direct unit tests', () => {
  it('removes trailing whitespace and normalizes CRLF', () => {
    const result = minifyGeneralCore('alpha  \r\nbeta\t\r\n');

    expect(result).toBe('alpha\nbeta');
    expect(result).not.toContain('\r\n');
  });

  it('collapses 3+ blank lines to one blank spacer', () => {
    const result = minifyGeneralCore('alpha\n\n\n\nbeta');

    expect(result).toBe('alpha\n\nbeta');
  });

  it('halves leading spaces with Math.ceil while keeping at least one space', () => {
    const result = minifyGeneralCore('root\n  two\n   three\n    four');

    expect(result).toBe('root\n two\n  three\n  four');
  });

  it('treats each tab as four spaces for indentation compression', () => {
    const result = minifyGeneralCore('root\n\tTabbed\n\t\tDouble');

    expect(result).toBe('root\n  Tabbed\n    Double');
  });

  it('trims leading and trailing blank content', () => {
    const result = minifyGeneralCore('\n\n  value  \n\n');

    expect(result).toBe('value');
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 8. SIGNATURES_ONLY_HINT constant
// ─────────────────────────────────────────────────────────────────────────────

describe('SIGNATURES_ONLY_HINT', () => {
  it('is a non-empty string', () => {
    expect(typeof SIGNATURES_ONLY_HINT).toBe('string');
    expect(SIGNATURES_ONLY_HINT.length).toBeGreaterThan(0);
  });

  it('mentions signatures/bodies/line numbers — the key agent affordances', () => {
    const hint = SIGNATURES_ONLY_HINT.toLowerCase();
    expect(hint).toContain('signature');
    expect(hint).toContain('line');
    expect(hint).toContain('paginated');
    expect(hint).toContain('startline');
    expect(hint).toContain('endline');
  });
});
