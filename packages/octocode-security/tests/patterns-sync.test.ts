/**
 * patterns-sync.test.ts
 *
 * Verifies that src/patterns.rs (the auto-generated Rust file) is in sync with
 * the canonical TypeScript pattern source in src/regexes/.
 *
 * This test is the CI equivalent of `yarn verify:patterns`.  It catches the
 * case where someone edits src/regexes/*.ts but forgets to run `yarn gen`
 * before committing, which would cause the Rust binary to ship stale patterns.
 *
 * Since `build:ts` now calls `gen-patterns.mjs` first, this test also acts as
 * a regression guard for that gate.
 *
 * Layout:
 *   SYNC-01  Pattern count parity (TS ↔ patterns.rs ↔ native binary)
 *   SYNC-02  Pattern names present and in canonical TS order
 *   SYNC-03  File-context metadata parity
 *   SYNC-04  No hand-edited mutations in patterns.rs
 *   SYNC-05  build:ts script includes gen-patterns.mjs
 */

import { describe, it, expect, beforeAll } from 'vitest';
import { readFileSync } from 'fs';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';
import { allRegexPatterns } from '../src/regexes/index.js';
import { nativePatternCount } from '../src/native.js';

const __dir = dirname(fileURLToPath(import.meta.url));
const PATTERNS_RS = resolve(__dir, '..', 'src', 'patterns.rs');
const PACKAGE_JSON = resolve(__dir, '..', 'package.json');

// ---------------------------------------------------------------------------
// Parse patterns.rs once for all tests.
// ---------------------------------------------------------------------------
let rsSource = '';
beforeAll(() => {
  rsSource = readFileSync(PATTERNS_RS, 'utf8');
});

// ---------------------------------------------------------------------------
// SYNC-01: Count parity
// ---------------------------------------------------------------------------
describe('SYNC-01: Pattern count parity', () => {
  it('allRegexPatterns (TS source) count matches patterns.rs Pattern entries', () => {
    // Count `Pattern { name:` — excludes the struct declaration line.
    const rsCount = (rsSource.match(/Pattern \{ name:/g) ?? []).length;
    expect(rsCount).toBe(allRegexPatterns.length);
  });

  it('allRegexPatterns count matches nativePatternCount() (compiled binary)', () => {
    expect(nativePatternCount()).toBe(allRegexPatterns.length);
  });

  it('patterns.rs count matches nativePatternCount() (compiled binary)', () => {
    const rsCount = (rsSource.match(/Pattern \{ name:/g) ?? []).length;
    expect(rsCount).toBe(nativePatternCount());
  });
});

// ---------------------------------------------------------------------------
// SYNC-02: Pattern name order
// ---------------------------------------------------------------------------
describe('SYNC-02: Pattern names in patterns.rs match TS canonical order', () => {
  it('every TS pattern name appears in patterns.rs', () => {
    for (const p of allRegexPatterns) {
      expect(rsSource, `missing pattern "${p.name}"`).toContain(
        `name: "${p.name}"`
      );
    }
  });

  it('names appear in the same order as allRegexPatterns', () => {
    // Extract ordered names from patterns.rs
    const nameRegex = /name:\s*"([^"]+)"/g;
    const rsNames: string[] = [];
    let m: RegExpExecArray | null;
    while ((m = nameRegex.exec(rsSource)) !== null) {
      rsNames.push(m[1]!);
    }

    const tsNames = allRegexPatterns.map(p => p.name);
    expect(rsNames).toEqual(tsNames);
  });
});

// ---------------------------------------------------------------------------
// SYNC-03: File-context metadata parity
// ---------------------------------------------------------------------------
describe('SYNC-03: file_context metadata parity', () => {
  it('patterns with fileContext in TS have file_context: Some(...) in patterns.rs', () => {
    const withCtx = allRegexPatterns.filter(p => p.fileContext);
    for (const p of withCtx) {
      // The pattern block in patterns.rs should NOT have file_context: None
      // for this name.  Find the block and check.
      const blockStart = rsSource.indexOf(`name: "${p.name}"`);
      expect(
        blockStart,
        `pattern "${p.name}" not found in patterns.rs`
      ).toBeGreaterThan(-1);
      const blockEnd = rsSource.indexOf('}', blockStart);
      const block = rsSource.slice(blockStart, blockEnd + 1);
      expect(
        block,
        `"${p.name}" should have file_context: Some(...)`
      ).toContain('file_context: Some(');
    }
  });

  it('patterns without fileContext in TS have file_context: None in patterns.rs', () => {
    const withoutCtx = allRegexPatterns.filter(p => !p.fileContext);
    for (const p of withoutCtx) {
      const blockStart = rsSource.indexOf(`name: "${p.name}"`);
      expect(blockStart).toBeGreaterThan(-1);
      const blockEnd = rsSource.indexOf('}', blockStart);
      const block = rsSource.slice(blockStart, blockEnd + 1);
      expect(block, `"${p.name}" should have file_context: None`).toContain(
        'file_context: None'
      );
    }
  });

  it('fileContext count matches between TS and patterns.rs', () => {
    const tsCtxCount = allRegexPatterns.filter(p => p.fileContext).length;
    const rsCtxCount = (rsSource.match(/file_context: Some\(/g) ?? []).length;
    expect(rsCtxCount).toBe(tsCtxCount);
  });
});

// ---------------------------------------------------------------------------
// SYNC-04: No hand-edited mutations
// ---------------------------------------------------------------------------
describe('SYNC-04: patterns.rs integrity', () => {
  it('has the DO NOT EDIT header comment', () => {
    expect(rsSource).toContain('AUTO-GENERATED by scripts/gen-patterns.mjs');
    expect(rsSource).toContain('DO NOT EDIT');
  });

  it('PATTERNS static slice length matches pattern count', () => {
    // The generated file declares `pub static PATTERNS: &[Pattern] = &[...]`.
    // The number of Pattern entries inside must equal allRegexPatterns.length.
    const rsCount = (rsSource.match(/Pattern \{ name:/g) ?? []).length;
    expect(rsCount).toBe(allRegexPatterns.length);
  });

  it('contains REGEX_SET builder (not missing from generation)', () => {
    expect(rsSource).toContain('RegexSetBuilder');
    expect(rsSource).toContain('REGEX_SET');
  });

  it('contains PATTERN_REGEXES lazy static (not missing from generation)', () => {
    expect(rsSource).toContain('PATTERN_REGEXES');
  });
});

// ---------------------------------------------------------------------------
// SYNC-05: build:ts script includes gen-patterns.mjs
// ---------------------------------------------------------------------------
describe('SYNC-05: build:ts script gate', () => {
  it('package.json build:ts script calls gen-patterns.mjs before building', () => {
    const pkg = JSON.parse(readFileSync(PACKAGE_JSON, 'utf8')) as {
      scripts: Record<string, string>;
    };
    const buildTs = pkg.scripts['build:ts'] ?? '';
    expect(buildTs).toContain('gen-patterns.mjs');
    // gen-patterns must appear BEFORE build.mjs (left-to-right execution)
    const genIdx = buildTs.indexOf('gen-patterns.mjs');
    const buildIdx = buildTs.indexOf('build.mjs');
    expect(genIdx).toBeLessThan(buildIdx);
  });

  it('build:rust script also calls gen-patterns.mjs', () => {
    const pkg = JSON.parse(readFileSync(PACKAGE_JSON, 'utf8')) as {
      scripts: Record<string, string>;
    };
    const buildRust = pkg.scripts['build:rust'] ?? '';
    expect(buildRust).toContain('gen-patterns.mjs');
  });

  it('build:dev script also calls gen-patterns.mjs', () => {
    const pkg = JSON.parse(readFileSync(PACKAGE_JSON, 'utf8')) as {
      scripts: Record<string, string>;
    };
    const buildDev = pkg.scripts['build:dev'] ?? '';
    expect(buildDev).toContain('gen-patterns.mjs');
  });
});
