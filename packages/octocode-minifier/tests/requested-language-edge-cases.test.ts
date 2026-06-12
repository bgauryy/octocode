import { describe, expect, it } from 'vitest';
import {
  applyContentViewMinification,
  applyMinification,
  minifyContent,
  minifyContentSync,
  MINIFY_CONFIG,
} from '@octocodeai/octocode-minifier';

type LanguageExpectation = {
  readonly language: string;
  readonly extensions: readonly string[];
};

type EdgeCase = {
  readonly language: string;
  readonly filePath: string;
  readonly content: string;
  readonly keep: readonly string[];
  readonly drop: readonly string[];
};

const REQUESTED_LANGUAGE_EXPECTATIONS: readonly LanguageExpectation[] = [
  { language: 'Python', extensions: ['py'] },
  { language: 'C', extensions: ['c', 'h'] },
  { language: 'C++', extensions: ['cpp', 'cc', 'hpp'] },
  { language: 'Java', extensions: ['java'] },
  { language: 'C#', extensions: ['cs'] },
  { language: 'JavaScript', extensions: ['js', 'jsx', 'mjs', 'cjs'] },
  { language: 'Visual Basic', extensions: ['vb', 'vbs'] },
  { language: 'SQL', extensions: ['sql', 'tsql', 'plsql'] },
  { language: 'HTML', extensions: ['html', 'htm'] },
  { language: 'TypeScript', extensions: ['ts', 'tsx'] },
];

const EDGE_CASES: readonly EdgeCase[] = [
  {
    language: 'Python',
    filePath: 'edge.py',
    content: `#!/usr/bin/env python3
"""DROP_MARKER_PY_DOCSTRING"""
def load_query(path: str) -> str:
    value = "# PY_KEEP_MARKER hash inside string"
    sql = """SELECT '-- PY_KEEP_MARKER sql marker' AS value"""
    # DROP_MARKER_PY_HASH
    return value + path + sql
`,
    keep: ['PY_KEEP_MARKER hash inside string', 'PY_KEEP_MARKER sql marker'],
    drop: ['DROP_MARKER_PY_DOCSTRING', 'DROP_MARKER_PY_HASH'],
  },
  {
    language: 'C',
    filePath: 'edge.c',
    content: `#include <stdio.h>
/* DROP_MARKER_C_BLOCK */
int main(void) {
    const char *url = "https://example.com/c//C_KEEP_MARKER";
    const char *text = "/* C_KEEP_MARKER block-looking string */";
    puts(url); // DROP_MARKER_C_LINE
    puts(text);
    return 0;
}
`,
    keep: ['C_KEEP_MARKER', 'block-looking string'],
    drop: ['DROP_MARKER_C_BLOCK', 'DROP_MARKER_C_LINE'],
  },
  {
    language: 'C++',
    filePath: 'edge.cpp',
    content: `#include <string>
// DROP_MARKER_CPP_LINE
std::string render() {
  const std::string url = "https://example.com/cpp//CPP_KEEP_MARKER";
  const std::string raw = R"(CPP_KEEP_MARKER /* keep */ // keep)";
  return url + raw;
}
`,
    keep: ['CPP_KEEP_MARKER'],
    drop: ['DROP_MARKER_CPP_LINE'],
  },
  {
    language: 'Java',
    filePath: 'Edge.java',
    content: `public final class Edge {
  /* DROP_MARKER_JAVA_BLOCK */
  public String render() {
    String url = "https://example.com/java//JAVA_KEEP_MARKER";
    return url; // DROP_MARKER_JAVA_LINE
  }
}
`,
    keep: ['JAVA_KEEP_MARKER'],
    drop: ['DROP_MARKER_JAVA_BLOCK', 'DROP_MARKER_JAVA_LINE'],
  },
  {
    language: 'C#',
    filePath: 'Edge.cs',
    content: `public static class Edge {
  // DROP_MARKER_CS_LINE
  public static string Render() {
    var url = "https://example.com/cs//CS_KEEP_MARKER";
    var verbatim = @"C:\\tmp\\CS_KEEP_MARKER\\file";
    return url + verbatim;
  }
}
`,
    keep: ['CS_KEEP_MARKER'],
    drop: ['DROP_MARKER_CS_LINE'],
  },
  {
    language: 'JavaScript',
    filePath: 'edge.js',
    content: `// DROP_MARKER_JS_LINE
const url = "https://example.com/js//JS_KEEP_MARKER";
const pattern = /https?:\\/\\/example\\.com\\/JS_KEEP_MARKER/;
export function render(value) {
  return pattern.test(value) ? url : "JS_KEEP_MARKER";
}
`,
    keep: ['JS_KEEP_MARKER'],
    drop: ['DROP_MARKER_JS_LINE'],
  },
  {
    language: 'JSX',
    filePath: 'Edge.jsx',
    content: `import React from 'react';
// DROP_MARKER_JSX_LINE
export function Edge({ label }) {
  return (
    <section data-url="https://example.com/jsx//JSX_KEEP_MARKER">
      {/* DROP_MARKER_JSX_BLOCK */}
      <span>{label ?? 'JSX_KEEP_MARKER'}</span>
    </section>
  );
}
`,
    keep: ['JSX_KEEP_MARKER'],
    drop: ['DROP_MARKER_JSX_LINE', 'DROP_MARKER_JSX_BLOCK'],
  },
  {
    language: 'TypeScript',
    filePath: 'edge.ts',
    content: `// DROP_MARKER_TS_LINE
type Payload = { readonly value: string };
export function render(payload: Payload): string {
  const url: string = "https://example.com/ts//TS_KEEP_MARKER";
  return payload.value + url;
}
`,
    keep: ['TS_KEEP_MARKER'],
    drop: ['DROP_MARKER_TS_LINE'],
  },
  {
    language: 'TSX',
    filePath: 'Edge.tsx',
    content: `import React from 'react';
// DROP_MARKER_TSX_LINE
type Props = { readonly title?: string };
export function Edge({ title = 'TSX_KEEP_MARKER' }: Props) {
  return <article data-url="https://example.com/tsx//TSX_KEEP_MARKER">{title}</article>;
}
`,
    keep: ['TSX_KEEP_MARKER'],
    drop: ['DROP_MARKER_TSX_LINE'],
  },
  {
    language: 'Visual Basic',
    filePath: 'Edge.vb',
    content: `Option Strict On
' DROP_MARKER_VB_LINE
Public Module Edge
  Public Function Render() As String
    Dim value As String = "' VB_KEEP_MARKER apostrophe inside string"
    Return value
  End Function
End Module
`,
    keep: ['VB_KEEP_MARKER apostrophe inside string'],
    drop: ['DROP_MARKER_VB_LINE'],
  },
  {
    language: 'SQL',
    filePath: 'edge.sql',
    content: `/* DROP_MARKER_SQL_BLOCK */
SELECT '-- SQL_KEEP_MARKER line-looking string' AS value
FROM accounts
WHERE note = 'https://example.com/sql--SQL_KEEP_MARKER'; -- DROP_MARKER_SQL_LINE
`,
    keep: ['SQL_KEEP_MARKER line-looking string', 'SQL_KEEP_MARKER'],
    drop: ['DROP_MARKER_SQL_BLOCK', 'DROP_MARKER_SQL_LINE'],
  },
  {
    language: 'HTML',
    filePath: 'edge.html',
    content: `<!doctype html>
<!-- DROP_MARKER_HTML_COMMENT -->
<html>
  <body>
    <script>
      const marker = "<!-- HTML_KEEP_MARKER string comment -->";
    </script>
    <div data-template="<!-- HTML_KEEP_MARKER attribute -->">HTML_KEEP_MARKER</div>
  </body>
</html>
`,
    keep: ['HTML_KEEP_MARKER'],
    drop: ['DROP_MARKER_HTML_COMMENT'],
  },
];

const PARTIAL_CASES: readonly EdgeCase[] = [
  {
    language: 'Partial TSX',
    filePath: 'partial.tsx',
    content: `return <Panel title="// PARTIAL_TSX_KEEP">
  <Item href="https://example.com/partial//PARTIAL_TSX_KEEP">
`,
    keep: ['PARTIAL_TSX_KEEP'],
    drop: [],
  },
  {
    language: 'Partial SQL',
    filePath: 'partial.sql',
    content: `SELECT '-- PARTIAL_SQL_KEEP' AS marker
FROM users
WHERE note LIKE '%-- PARTIAL_SQL_KEEP%'
-- DROP_MARKER_PARTIAL_SQL
`,
    keep: ['PARTIAL_SQL_KEEP'],
    drop: ['DROP_MARKER_PARTIAL_SQL'],
  },
  {
    language: 'Partial C',
    filePath: 'partial.c',
    content: `int run(void) {
  const char *url = "https://example.com/partial//PARTIAL_C_KEEP";
  return url != 0;
`,
    keep: ['PARTIAL_C_KEEP'],
    drop: [],
  },
];

function expectMarkers(output: string, testCase: EdgeCase): void {
  expect(output.length, `${testCase.language} output length`).toBeGreaterThan(
    0
  );

  for (const marker of testCase.keep) {
    expect(output, `${testCase.language} keep ${marker}`).toContain(marker);
  }

  for (const marker of testCase.drop) {
    expect(output, `${testCase.language} drop ${marker}`).not.toContain(marker);
  }
}

describe('requested language minifier audit', () => {
  it.each(REQUESTED_LANGUAGE_EXPECTATIONS)(
    'has explicit minifier config for $language',
    expectation => {
      for (const ext of expectation.extensions) {
        expect(MINIFY_CONFIG.fileTypes[ext], ext).toBeDefined();
      }
    }
  );

  it.each(EDGE_CASES)(
    'preserves string markers while stripping comments for $language',
    testCase => {
      const contentView = applyContentViewMinification(
        testCase.content,
        testCase.filePath
      );
      const sync = minifyContentSync(testCase.content, testCase.filePath);
      const guarded = applyMinification(testCase.content, testCase.filePath);

      expectMarkers(contentView, testCase);
      expectMarkers(sync, testCase);
      expectMarkers(guarded, testCase);
    }
  );

  it.each(PARTIAL_CASES)(
    'handles partial $language snippets without throwing',
    testCase => {
      expect(() =>
        applyContentViewMinification(testCase.content, testCase.filePath)
      ).not.toThrow();
      expect(() =>
        minifyContentSync(testCase.content, testCase.filePath)
      ).not.toThrow();

      expectMarkers(
        applyContentViewMinification(testCase.content, testCase.filePath),
        testCase
      );
    }
  );

  it('keeps async minification bounded for very large files', async () => {
    const corpus = EDGE_CASES.map(testCase => testCase.content).join('\n');
    const repetitions = Math.ceil((1024 * 1024 * 1.1) / corpus.length);
    const largeContent = corpus.repeat(repetitions);
    const result = await minifyContent(largeContent, 'huge.ts');

    expect(result.failed).toBe(true);
    expect(result.type).toBe('failed');
    expect(result.content).toBe(largeContent);
    expect(result.reason).toContain('exceeds 1MB limit');
  });
});
