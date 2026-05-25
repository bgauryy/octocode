/**
 * Unified-hint-flow contract.
 *
 * Single source of truth for tool hints, asserted in one place:
 *
 *   Tool description = upstream description + optional one-time meta
 *                      (vsLocal / nameVariants / etc). NO hint blocks,
 *                      NO empty-recovery prose, NO per-call narration.
 *
 *   Per-response hints come from EXACTLY ONE flow:
 *     - dynamic empty branch    (query-shape aware recovery)
 *     - dynamic error branch    (failure-specific recovery)
 *     - pagination cursor       (only when more pages exist)
 *
 *   Server-level MCP instructions = upstream content.instructions ONLY.
 *   No `genericHints` coach block.
 */

import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { hints as codeHints } from '../../src/tools/github_search_code/hints.js';
import { hints as fetchHints } from '../../src/tools/github_fetch_content/hints.js';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = join(HERE, '../../src/tools');
const SRC = join(HERE, '../../src');

describe('tool registrations contain no <hints> block', () => {
  // The describeWithHints / GENERAL_HINTS wrapper was removed; the two
  // affected files must not re-introduce it.
  const files = [
    'github_search_code/github_search_code.ts',
    'github_fetch_content/github_fetch_content.ts',
  ];

  it.each(files)('%s has no static <hints> block in its description', file => {
    const src = readFileSync(join(ROOT, file), 'utf8');
    expect(src).not.toMatch(/describeWithHints/);
    expect(src).not.toMatch(/GENERAL_HINTS/);
    expect(src).not.toMatch(/<hints>/);
  });
});

describe('local-tool description meta — one-time disambiguation', () => {
  it('localSearchCode register appends a <vsLocal> note', () => {
    const src = readFileSync(join(ROOT, 'local_ripgrep/register.ts'), 'utf8');
    expect(src).toMatch(/<vsLocal>/);
    expect(src).toMatch(/localFindFiles/);
    expect(src).toMatch(/ripgrep is for content/);
  });

  it('localFindFiles register appends a <nameVariants> block', () => {
    const src = readFileSync(
      join(ROOT, 'local_find_files/register.ts'),
      'utf8'
    );
    expect(src).toMatch(/<nameVariants>/);
    expect(src).toMatch(/iname/);
    expect(src).toMatch(/pathPattern/);
    expect(src).toMatch(/names:/);
  });
});

describe('server-level instructions — no genericHints block', () => {
  it('src/index.ts does not declare a genericHints array', () => {
    const src = readFileSync(join(SRC, 'index.ts'), 'utf8');
    expect(src).not.toMatch(/genericHints/);
    expect(src).not.toMatch(/Got 3\+ examples\?/);
    expect(src).not.toMatch(/Try broader terms/);
    expect(src).not.toMatch(/If stuck in loop/);
  });
});

describe('dynamic empty branches now carry the moved recovery hints', () => {
  it('githubSearchCode empty + single packagey keyword → packageSearch redirect', () => {
    const hints = codeHints.empty!({
      hasOwnerRepo: false,
      keywords: ['react-router'],
    } as Record<string, unknown>);
    const flat = hints.filter((s): s is string => typeof s === 'string');
    expect(flat.some(s => s.includes('packageSearch'))).toBe(true);
    expect(flat.some(s => s.includes('react-router'))).toBe(true);
  });

  it('githubSearchCode empty + non-packagey query does NOT redirect to packageSearch', () => {
    const hints = codeHints.empty!({
      hasOwnerRepo: false,
      keywords: ['has spaces and weird stuff'],
    } as Record<string, unknown>);
    const flat = hints.filter((s): s is string => typeof s === 'string');
    expect(flat.some(s => s.includes('packageSearch'))).toBe(false);
  });

  it('githubGetFileContent empty + path → verify with viewRepoStructure', () => {
    const hints = fetchHints.empty!({ path: 'src/x.ts' } as Record<
      string,
      unknown
    >);
    const flat = hints.filter((s): s is string => typeof s === 'string');
    expect(flat.some(s => s.includes("'src/x.ts'"))).toBe(true);
    expect(flat.some(s => s.includes('githubViewRepoStructure'))).toBe(true);
  });

  it('githubGetFileContent error + not_found names path and branch', () => {
    const hints = fetchHints.error!({
      errorType: 'not_found',
      path: 'src/missing.ts',
      branch: 'feat/x',
    } as Record<string, unknown>);
    const flat = hints.filter((s): s is string => typeof s === 'string');
    expect(flat.some(s => s.includes('src/missing.ts'))).toBe(true);
    expect(flat.some(s => s.includes('feat/x'))).toBe(true);
  });
});
