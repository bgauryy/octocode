/**
 * docs-catalog.test.ts — unit tests for skill-reference list/show.
 */

import { describe, it, expect } from 'vitest';
import { existsSync, mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { listSkillDocs, showSkillDoc } from '../src/docs-catalog.js';

describe('docs-catalog', () => {
  it('lists skill reference markdown files with name/title/description', () => {
    const result = listSkillDocs();
    expect(result.ok).toBe(true);
    expect(result.count).toBeGreaterThan(0);
    expect(existsSync(result.root)).toBe(true);
    const names = result.docs.map((doc) => doc.name);
    expect(names).toEqual(expect.arrayContaining(['full-flow', 'agent-cheatsheet', 'hooks']));
    for (const doc of result.docs) {
      expect(doc.kind).toBe('skill-ref');
      expect(doc.name.length).toBeGreaterThan(0);
      expect(doc.title.length).toBeGreaterThan(0);
      expect(doc.description.length).toBeGreaterThan(0);
      expect(doc.path.endsWith(`${doc.name}.md`)).toBe(true);
    }
  });

  it('shows a known doc by name and returns content', () => {
    const result = showSkillDoc('full-flow');
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.name).toBe('full-flow');
    expect(result.content).toContain('#');
    expect(result.kind).toBe('skill-ref');
  });

  it('returns suggestions for unknown names', () => {
    const result = showSkillDoc('no-such-doc-xyz');
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toContain('docs list');
    expect(result.suggestions.length).toBeGreaterThan(0);
  });

  it('reads from an explicit root override', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-docs-catalog-'));
    try {
      mkdirSync(dir, { recursive: true });
      writeFileSync(join(dir, 'sample.md'), '# Sample Doc\n\nFirst paragraph about sample.\n', 'utf8');
      const listed = listSkillDocs({ root: dir });
      expect(listed.count).toBe(1);
      expect(listed.docs[0]?.name).toBe('sample');
      expect(listed.docs[0]?.title).toBe('Sample Doc');
      expect(listed.docs[0]?.description).toContain('First paragraph');
      const shown = showSkillDoc('sample', { root: dir });
      expect(shown.ok).toBe(true);
      if (!shown.ok) return;
      expect(shown.content).toContain('Sample Doc');
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
