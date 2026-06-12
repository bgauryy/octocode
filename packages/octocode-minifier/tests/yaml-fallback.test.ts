// Tests for jsonToYamlString error-recovery paths (lines 97-103 in yaml source).
// Kept in a separate file without any vi.mock() so v8 coverage can track
// the error-handling code paths accurately.

import { describe, it, expect } from 'vitest';
import { jsonToYamlString } from '@octocodeai/octocode-minifier';

describe('jsonToYamlString — double-fallback (circular / non-serialisable)', () => {
  it('returns an error comment when both js-yaml AND JSON.stringify fail', () => {
    // Circular references cause js-yaml (noRefs:true) to recurse until stack
    // overflow, then JSON.stringify also throws → innermost catch fires.
    const circular: Record<string, unknown> = {};
    circular.self = circular;

    const result = jsonToYamlString(circular);

    expect(typeof result).toBe('string');
    expect(result).toContain('# YAML conversion failed:');
    expect(result).toContain('# JSON conversion also failed:');
    expect(result).toContain('# Object: [Unconvertible]');
  });

  it('includes both error messages in the fallback comment', () => {
    const a: Record<string, unknown> = {};
    const b: Record<string, unknown> = { back: a };
    a.fwd = b;

    const result = jsonToYamlString(a);

    expect(result).toMatch(/# YAML conversion failed: .+/);
    expect(result).toMatch(/# JSON conversion also failed: .+/);
  });

  it('does not throw for circular structures', () => {
    const cycle: Record<string, unknown> = { x: 1 };
    cycle.loop = cycle;

    expect(() => jsonToYamlString(cycle)).not.toThrow();
  });

  it('returns a string (never undefined) for circular structures', () => {
    const circ: Record<string, unknown> = {};
    circ.circ = circ;

    const result = jsonToYamlString(circ);
    expect(typeof result).toBe('string');
    expect(result.length).toBeGreaterThan(0);
  });
});

// ---------------------------------------------------------------------------
// minifyMarkdownCore — new strip rules (badges, pseudo-comments, setext, HR)
// ---------------------------------------------------------------------------

import { minifyMarkdownCore } from '@octocodeai/octocode-minifier';

describe('minifyMarkdownCore — badge and pseudo-comment stripping', () => {
  it('strips shield.io badge lines', () => {
    const input = [
      '![CI](https://github.com/org/repo/workflows/ci/badge.svg)',
      '![Coverage](https://img.shields.io/codecov/c/github/org/repo)',
      '![npm](https://img.shields.io/npm/v/pkg.svg)',
      '',
      '# My Project',
    ].join('\n');
    const result = minifyMarkdownCore(input);
    expect(result).not.toContain('shields.io');
    expect(result).not.toContain('badge.svg');
    expect(result).toContain('# My Project');
  });

  it('strips link-wrapped badge-only lines', () => {
    const input = [
      '[![Build](https://img.shields.io/github/actions/workflow/status/org/repo/ci.yml)](https://github.com/org/repo/actions) [![npm](https://badge.fury.io/js/pkg.svg)](https://badge.fury.io/js/pkg)',
      '[![Coverage](https://codecov.io/gh/org/repo/branch/main/graph/badge.svg)](https://codecov.io/gh/org/repo)',
      '',
      '# My Project',
      '',
      '[regular link](https://example.com)',
    ].join('\n');
    const result = minifyMarkdownCore(input);
    expect(result).not.toContain('img.shields.io');
    expect(result).not.toContain('badge.fury.io');
    expect(result).not.toContain('codecov.io/gh/org/repo/branch');
    expect(result).toContain('# My Project');
    expect(result).toContain('[regular link](https://example.com)');
  });

  it('keeps relative/local image references', () => {
    const input = '![diagram](./docs/arch.png)\n\n# Heading';
    const result = minifyMarkdownCore(input);
    expect(result).toContain('docs/arch.png');
    expect(result).toContain('# Heading');
  });

  it('keeps non-badge remote image references', () => {
    const input =
      '![architecture](https://example.com/docs/architecture.png)\n\n# Heading';
    const result = minifyMarkdownCore(input);
    expect(result).toContain('https://example.com/docs/architecture.png');
    expect(result).toContain('# Heading');
  });

  it('strips [//]: # pseudo-comments', () => {
    const input = [
      '[//]: # (This is a hidden comment)',
      '[//]: # "Another hidden note"',
      '',
      'Visible paragraph.',
    ].join('\n');
    const result = minifyMarkdownCore(input);
    expect(result).not.toContain('[//]:');
    expect(result).toContain('Visible paragraph.');
  });

  it('converts setext H1 underlines to ATX headings', () => {
    const input = 'My Heading\n==========\n\nParagraph text.';
    const result = minifyMarkdownCore(input);
    expect(result).not.toMatch(/^={3,}/m);
    expect(result).toContain('# My Heading');
    expect(result).toContain('Paragraph text.');
  });

  it('converts multiline setext H2 headings without orphaning text', () => {
    const input = 'My Heading\nWrapped Line\n---\n\nParagraph text.';
    const result = minifyMarkdownCore(input);
    expect(result).toContain('## My Heading Wrapped Line');
    expect(result).not.toContain('My Heading\nWrapped Line');
    expect(result).toContain('Paragraph text.');
  });

  it('converts *** horizontal rules to shortest thematic break', () => {
    const input = 'Section A\n\n***\n\nSection B';
    const result = minifyMarkdownCore(input);
    expect(result).not.toMatch(/^\*{3,}/m);
    expect(result).toContain('---');
    expect(result).toContain('Section A');
    expect(result).toContain('Section B');
  });

  it('converts ___ horizontal rules to shortest thematic break', () => {
    const input = 'Above\n\n___\n\nBelow';
    const result = minifyMarkdownCore(input);
    expect(result).not.toMatch(/^_{3,}/m);
    expect(result).toContain('---');
    expect(result).toContain('Above');
    expect(result).toContain('Below');
  });

  it('does NOT strip --- (may be YAML frontmatter or setext H2)', () => {
    const input = '---\ntitle: Post\n---\n\nContent.';
    const result = minifyMarkdownCore(input);
    // --- lines are preserved
    expect(result).toContain('---');
  });

  it('compacts GFM tables without removing cell text', () => {
    const input = [
      '| Name   | Value      | Notes |',
      '| :----- | ----------: | :---: |',
      '| alpha  | 100        | ok    |',
      '| beta   | 200        | fine  |',
    ].join('\n');
    const result = minifyMarkdownCore(input);
    expect(result).toContain('|Name|Value|Notes|');
    expect(result).toContain('|:-----|----------:|:---:|');
    expect(result).toContain('|alpha|100|ok|');
  });

  it('compacts task lists and nested blockquotes while preserving meaning', () => {
    const input = [
      '-    [x]      completed item',
      '-    [ ]      pending item',
      '',
      '>    Important note',
      '> >       Nested note',
    ].join('\n');
    const result = minifyMarkdownCore(input);
    expect(result).toContain('- [x] completed item');
    expect(result).toContain('- [ ] pending item');
    expect(result).toContain('> Important note');
    expect(result).toContain('> > Nested note');
  });

  it('preserves fenced code block contents exactly enough for comments and pipes', () => {
    const input = [
      'Before',
      '',
      '```ts',
      'const value = "<!-- keep -->";',
      'const row = "a | b | c";',
      '',
      '```',
      '',
      'After',
    ].join('\n');
    const result = minifyMarkdownCore(input);
    expect(result).toContain('const value = "<!-- keep -->";');
    expect(result).toContain('const row = "a | b | c";');
    expect(result).toContain('```ts');
  });

  it('turns two-space hard breaks into CommonMark backslash hard breaks', () => {
    const input = 'First line  \nSecond line';
    const result = minifyMarkdownCore(input);
    expect(result).toContain('First line\\\nSecond line');
  });

  it('produces measurable savings on a badge-heavy README', () => {
    const badges = Array.from(
      { length: 10 },
      (_, i) => `![badge${i}](https://img.shields.io/badge/x-y-green.svg)`
    ).join('\n');
    const body = '\n\n# Title\n\nDescription text.\n';
    const input = badges + body;
    const result = minifyMarkdownCore(input);
    expect(result.length).toBeLessThan(input.length * 0.5);
    expect(result).toContain('# Title');
  });

  it('strips multiple HTML block comments', () => {
    const input = '<!-- TOC -->\n- [a](#a)\n<!-- /TOC -->\n\n# a\n\ntext';
    const result = minifyMarkdownCore(input);
    expect(result).not.toContain('<!-- TOC -->');
    expect(result).not.toContain('<!-- /TOC -->');
    expect(result).not.toContain('- [a](#a)');
    expect(result).toContain('# a');
  });

  it('strips generated TOC blocks without removing real headings', () => {
    const input = [
      '# Project',
      '',
      '<!-- START doctoc generated TOC please keep comment here to allow auto update -->',
      '- [Install](#install)',
      '- [Usage](#usage)',
      '<!-- END doctoc generated TOC please keep comment here to allow auto update -->',
      '',
      '## Install',
      '',
      "Don't remove apostrophes from prose or code.",
      '',
      '## Usage',
    ].join('\n');
    const result = minifyMarkdownCore(input);
    expect(result).not.toContain('START doctoc');
    expect(result).not.toContain('[Install](#install)');
    expect(result).toContain('## Install');
    expect(result).toContain("Don't remove apostrophes");
    expect(result).toContain('## Usage');
  });

  it('does not strip apostrophes from prose, link titles, or inline code', () => {
    const input = [
      "# Don't Break Text",
      '',
      "It's important to keep apostrophes.",
      '',
      '[docs](https://example.com "Bob\'s docs")',
      '',
      "`const text = 'keep me';`",
    ].join('\n');
    const result = minifyMarkdownCore(input);
    expect(result).toContain("# Don't Break Text");
    expect(result).toContain("It's important");
    expect(result).toContain('"Bob\'s docs"');
    expect(result).toContain("`const text = 'keep me';`");
  });

  it('never grows the output', () => {
    const long = Array.from(
      { length: 50 },
      (_, i) => `Paragraph number ${i} with some regular prose text.`
    ).join('\n\n');
    const result = minifyMarkdownCore(long);
    expect(result.length).toBeLessThanOrEqual(long.length);
  });
});
