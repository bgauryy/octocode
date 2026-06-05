import { describe, it, expect } from 'vitest';
import { parseSkillFrontmatter } from '../../../src/utils/parsers/frontmatter.js';

describe('parseSkillFrontmatter', () => {
  it('returns null when there is no frontmatter block', () => {
    expect(parseSkillFrontmatter('# Just a heading\n')).toBeNull();
  });

  it('parses plain single-line scalars', () => {
    const md = [
      '---',
      'name: my-skill',
      'description: Does a thing',
      '---',
      '',
    ].join('\n');
    expect(parseSkillFrontmatter(md)).toEqual({
      name: 'my-skill',
      description: 'Does a thing',
      category: undefined,
    });
  });

  it('strips surrounding quotes', () => {
    const md = [
      '---',
      'name: "quoted-skill"',
      "description: 'quoted desc'",
      '---',
    ].join('\n');
    const parsed = parseSkillFrontmatter(md);
    expect(parsed?.name).toBe('quoted-skill');
    expect(parsed?.description).toBe('quoted desc');
  });

  it('folds a ">" block scalar into a single line', () => {
    const md = [
      '---',
      'name: langchain-best-practices',
      'description: >',
      '  Use this skill when building LangChain or',
      '  LangGraph applications. Covers chains',
      '  and agents.',
      'category: dev',
      '---',
    ].join('\n');
    const parsed = parseSkillFrontmatter(md);
    expect(parsed?.name).toBe('langchain-best-practices');
    expect(parsed?.description).toBe(
      'Use this skill when building LangChain or LangGraph applications. Covers chains and agents.'
    );
    expect(parsed?.category).toBe('dev');
  });

  it('preserves newlines for a "|" literal block scalar', () => {
    const md = [
      '---',
      'name: literal-skill',
      'description: |',
      '  line one',
      '  line two',
      '---',
    ].join('\n');
    expect(parseSkillFrontmatter(md)?.description).toBe('line one\nline two');
  });

  it('handles block scalars with chomping indicators (>-)', () => {
    const md = [
      '---',
      'description: >-',
      '  folded with chomp',
      '  second line',
      '---',
    ].join('\n');
    expect(parseSkillFrontmatter(md)?.description).toBe(
      'folded with chomp second line'
    );
  });

  it('never returns the bare block indicator as the value', () => {
    const md = ['---', 'description: >', '  real text', '---'].join('\n');
    expect(parseSkillFrontmatter(md)?.description).not.toBe('>');
  });
});
