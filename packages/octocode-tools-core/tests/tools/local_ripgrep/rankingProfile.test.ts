import { describe, expect, it } from 'vitest';

import type { LocalSearchCodeFile } from '@octocodeai/octocode-core/types';
import {
  classifyPathRole,
  isLowSignalQueryPath,
  selectProfile,
} from '../../../src/tools/local_ripgrep/rankingProfile/rankingProfiles.js';
import { rankFiles } from '../../../src/tools/local_ripgrep/rankingProfile/rankingResults.js';
import {
  scoreFileWithRarity,
  type RankContext,
} from '../../../src/tools/local_ripgrep/rankingProfile/rankingScoring.js';

function file(
  path: string,
  matchCount: number,
  lines: string[]
): LocalSearchCodeFile {
  return {
    path,
    matchCount,
    matches: lines.map((value, i) => ({ line: i + 1, value })),
  };
}

const ctx = (over: Partial<RankContext> = {}): RankContext => ({
  queryPath: '/repo',
  keyword: 'fallback',
  ...over,
});

describe('selectProfile', () => {
  it('selects by file extension', () => {
    expect(selectProfile('/repo/src/a.ts').id).toBe('typescript');
    expect(selectProfile('/repo/src/a.rs').id).toBe('rust');
    expect(selectProfile('/repo/src/a.py').id).toBe('python');
    expect(selectProfile('/repo/src/a.go').id).toBe('go');
    expect(selectProfile('/repo/README.md').id).toBe('markdown');
    expect(selectProfile('/repo/x.yaml').id).toBe('yaml');
  });

  it('langType overrides extension; unknown falls back to generic', () => {
    expect(selectProfile('/repo/a.txt', 'rust').id).toBe('rust');
    expect(selectProfile('/repo/a.unknownext').id).toBe('generic');
  });

  it('explicit profile override wins over auto-detection', () => {
    expect(selectProfile('/repo/a.ts', undefined, 'python').id).toBe('python');
  });
});

describe('classifyPathRole', () => {
  it('classifies roles deterministically', () => {
    expect(classifyPathRole('/repo/src/auth.ts')).toBe('source');
    expect(classifyPathRole('/repo/src/auth.test.ts')).toBe('test');
    expect(classifyPathRole('/repo/dist/bundle.min.js')).toBe('generated');
    expect(classifyPathRole('/repo/docs/guide.md')).toBe('docs');
    expect(classifyPathRole('/repo/tsconfig.json')).toBe('config');
    expect(classifyPathRole('/repo/__fixtures__/sample.ts')).toBe('fixture');
  });

  // De-rigidify: source is ecosystem-agnostic, not "only under src/".
  it('treats code files outside src/ as source (Go/Rust/Python layouts)', () => {
    expect(classifyPathRole('/repo/cmd/server/main.go')).toBe('source');
    expect(classifyPathRole('/repo/crates/core/lib.rs')).toBe('source');
    expect(classifyPathRole('/repo/handlers.py')).toBe('source'); // flat layout
    expect(classifyPathRole('/repo/internal/svc/h.go')).toBe('source');
  });

  it('does not promote build-output code files to source', () => {
    expect(classifyPathRole('/repo/dist/app.js')).toBe('unknown');
    expect(classifyPathRole('/repo/node_modules/x/index.js')).toBe('unknown');
  });

  it('non-code files outside known dirs stay unknown', () => {
    expect(classifyPathRole('/repo/data.bin')).toBe('unknown');
  });
});

describe('scoreFileWithRarity — the motivating broad-term example', () => {
  // RFC example: searching `fallback` should rank the file where it is a
  // declaration in src/ above a file with many incidental occurrences in dist/.
  const declInSrc = file('/repo/src/util/fallback.ts', 1, [
    'export function fallback(value: string) {',
  ]);
  const incidentalInDist = file('/repo/dist/bundle.js', 8, [
    '// fallback to default',
    'const x = "fallback";',
    'log("fallback path")',
  ]);

  it('declaration in source outscores incidental matches in generated output', () => {
    const a = scoreFileWithRarity(declInSrc, ctx());
    const b = scoreFileWithRarity(incidentalInDist, ctx());
    expect(a.score).toBeGreaterThan(b.score);
  });

  it('explains the score via reasons', () => {
    const a = scoreFileWithRarity(declInSrc, ctx());
    expect(a.profile).toBe('typescript');
    expect(a.pathRole).toBe('source');
    expect(a.reasons.join(' ')).toMatch(/declaration line/);
    expect(a.reasons.join(' ')).toMatch(/query token in path/);
  });

  it('match count never dominates a strong declaration', () => {
    // 100 incidental hits must not beat 1 declaration hit.
    const many = file('/repo/dist/huge.js', 100, ['x = "fallback"']);
    const one = file('/repo/src/fallback.ts', 1, [
      'export function fallback() {',
    ]);
    const ranked = rankFiles([many, one], 'relevance', ctx());
    expect(ranked.files[0]?.path).toBe('/repo/src/fallback.ts');
  });

  it('adds candidate-local rare query token reasons when debug ranking is on', () => {
    const common = file('/repo/src/common.ts', 10, ['const fallback = 1']);
    const rare = file('/repo/src/rare.ts', 1, [
      'const rankEvidence = fallback;',
    ]);
    const ranked = rankFiles(
      [common, rare],
      'relevance',
      ctx({ keyword: 'fallback rankEvidence' }),
      { debug: true }
    );

    const reasons = ranked.debug?.get('/repo/src/rare.ts')?.reasons.join(' ');
    expect(reasons).toMatch(/rare query token: rankEvidence \(1\/2 files\)/);
    expect(
      ranked.debug?.get('/repo/src/common.ts')?.reasons.join(' ')
    ).not.toMatch(/rare query token/);
  });
});

describe('rankFiles — sort modes', () => {
  const f1 = file('/repo/src/a.ts', 5, ['x']);
  const f2 = file('/repo/src/b.ts', 9, ['y']);
  const f3 = file('/repo/src/c.ts', 1, ['z']);

  it('matchCount escape hatch preserves legacy count-first order', () => {
    const r = rankFiles([f1, f2, f3], 'matchCount', ctx());
    expect(r.files.map(f => f.path)).toEqual([
      '/repo/src/b.ts',
      '/repo/src/a.ts',
      '/repo/src/c.ts',
    ]);
  });

  it('path escape hatch sorts alphabetically', () => {
    const r = rankFiles([f3, f1, f2], 'path', ctx());
    expect(r.files.map(f => f.path)).toEqual([
      '/repo/src/a.ts',
      '/repo/src/b.ts',
      '/repo/src/c.ts',
    ]);
  });

  it('filesystem sorts preserve engine order', () => {
    const r = rankFiles([f2, f3, f1], 'modified', ctx());
    expect(r.files.map(f => f.path)).toEqual([
      '/repo/src/b.ts',
      '/repo/src/c.ts',
      '/repo/src/a.ts',
    ]);
  });
});

describe('rankFiles — sortReverse', () => {
  // Regression: `sortReverse` used to only take effect for filesystem sorts
  // (created/modified/accessed), where the native engine had already applied
  // it before `rankFiles` ever saw the results. `matchCount`, `path`, and
  // `relevance` each re-sort in TS and silently dropped the flag entirely —
  // `sortReverse:true` had no observable effect for those three modes.
  const f1 = file('/repo/src/a.ts', 5, ['x']);
  const f2 = file('/repo/src/b.ts', 9, ['y']);
  const f3 = file('/repo/src/c.ts', 1, ['z']);

  it('reverses the path escape hatch', () => {
    const r = rankFiles([f3, f1, f2], 'path', ctx(), { sortReverse: true });
    expect(r.files.map(f => f.path)).toEqual([
      '/repo/src/c.ts',
      '/repo/src/b.ts',
      '/repo/src/a.ts',
    ]);
  });

  it('reverses the matchCount escape hatch', () => {
    const r = rankFiles([f1, f2, f3], 'matchCount', ctx(), {
      sortReverse: true,
    });
    expect(r.files.map(f => f.path)).toEqual([
      '/repo/src/c.ts',
      '/repo/src/a.ts',
      '/repo/src/b.ts',
    ]);
  });

  it('reverses relevance-ranked order', () => {
    const forward = rankFiles([f1, f2, f3], 'relevance', ctx());
    const reversed = rankFiles([f1, f2, f3], 'relevance', ctx(), {
      sortReverse: true,
    });
    expect(reversed.files.map(f => f.path)).toEqual(
      [...forward.files.map(f => f.path)].reverse()
    );
  });

  it('does not double-reverse a filesystem sort the engine already reversed', () => {
    // The engine, not rankFiles, is responsible for filesystem-sort reverse —
    // rankFiles must pass its (already-reversed) order through unchanged.
    const alreadyReversedByEngine = [f1, f3, f2];
    const r = rankFiles(alreadyReversedByEngine, 'modified', ctx(), {
      sortReverse: true,
    });
    expect(r.files.map(f => f.path)).toEqual([
      '/repo/src/a.ts',
      '/repo/src/c.ts',
      '/repo/src/b.ts',
    ]);
  });
});

describe('rankFiles — determinism', () => {
  it('is stable across repeated runs (same input, same output)', () => {
    const files = [
      file('/repo/src/b.ts', 2, ['const fallback = 1']),
      file('/repo/src/a.ts', 2, ['const fallback = 1']),
      file('/repo/dist/c.js', 9, ['"fallback"']),
    ];
    const r1 = rankFiles(files, 'relevance', ctx());
    const r2 = rankFiles(files, 'relevance', ctx());
    expect(r1.files.map(f => f.path)).toEqual(r2.files.map(f => f.path));
  });

  it('ties break by matchCount then path', () => {
    // Two identical-score files: same content, differ only by path.
    const a = file('/repo/src/zzz.ts', 1, ['const fallback = 1']);
    const b = file('/repo/src/aaa.ts', 1, ['const fallback = 1']);
    const r = rankFiles([a, b], 'relevance', ctx());
    expect(r.files[0]?.path).toBe('/repo/src/aaa.ts');
  });
});

describe('rankFiles — candidate cap (pagination guard)', () => {
  it('caps scoring to top-K but still returns every file (never gates)', () => {
    const files = Array.from({ length: 50 }, (_, i) =>
      file(`/repo/src/f${String(i).padStart(3, '0')}.ts`, i + 1, [
        'const fallback = 1',
      ])
    );
    const r = rankFiles(files, 'relevance', ctx(), { candidateCap: 10 });
    // Only the top-K are relevance-scored...
    expect(r.cappedCandidates).toBe(40);
    // ...but ranking enriches order, it never drops files: the unscored tail
    // is appended so pagination can still reach all of them.
    expect(r.files.length).toBe(50);
    // The scored top-K (highest match counts 49..40) sort to the front.
    expect(r.files.slice(0, 10).every(f => (f.matchCount ?? 0) >= 40)).toBe(
      true
    );
    // No file is lost.
    expect(new Set(r.files.map(f => f.path)).size).toBe(50);
  });

  it('does not cap when under the limit', () => {
    const files = [file('/repo/src/a.ts', 1, ['fallback'])];
    const r = rankFiles(files, 'relevance', ctx(), { candidateCap: 10 });
    expect(r.cappedCandidates).toBe(0);
  });
});

describe('false-positive hardening — regex fallback (no engine kind)', () => {
  // Fix #2: a commented-out declaration must not score like a live one.
  it('commented-out declaration is weak, not a declaration', () => {
    const live = file('/repo/src/a.ts', 1, ['export function fallback() {}']);
    const commented = file('/repo/src/b.ts', 1, [
      '// export function fallback() {}',
    ]);
    const liveScore = scoreFileWithRarity(live, ctx());
    const commentedScore = scoreFileWithRarity(commented, ctx());
    expect(commentedScore.score).toBeLessThan(liveScore.score);
    expect(commentedScore.reasons.join(' ')).toMatch(/comment match \(weak\)/);
    expect(commentedScore.reasons.join(' ')).not.toMatch(/declaration line/);
  });

  // Fix #2: trailing comment — token after the comment marker is weak.
  it('token in a trailing comment is weak', () => {
    const f = file('/repo/src/a.ts', 1, [
      'doStuff(); // fallback handler here',
    ]);
    const s = scoreFileWithRarity(f, ctx());
    expect(s.reasons.join(' ')).toMatch(/comment match \(weak\)/);
  });

  // Fix #3: token inside a string literal is weak, not a declaration/export.
  it('token inside a string literal is weak', () => {
    const f = file('/repo/src/a.ts', 1, [
      'const msg = "export function fallback";',
    ]);
    const s = scoreFileWithRarity(f, ctx());
    expect(s.reasons.join(' ')).toMatch(/string literal \(weak\)/);
    expect(s.reasons.join(' ')).not.toMatch(/declaration line/);
  });

  // Fix #4: classify the matched line, not a context line that looks structural.
  it('does not inherit a declaration from a context line', () => {
    // matched token "fallback" is in the returned string; the function decl is
    // a context line that must NOT lend its declaration score to this match.
    const f = file('/repo/src/a.ts', 1, [
      'export function other() {\n  return "fallback";\n}',
    ]);
    const s = scoreFileWithRarity(f, ctx());
    expect(s.reasons.join(' ')).not.toMatch(/declaration line/);
  });

  // Fix #5: exactness only fires for a plain-identifier search.
  it('regex keyword does not get whole-word exactness', () => {
    const f = file('/repo/src/a.ts', 1, ['const fallback = 1']);
    const plain = scoreFileWithRarity(f, ctx({ keyword: 'fallback' }));
    const regexy = scoreFileWithRarity(f, ctx({ keyword: 'fall.*back' }));
    expect(plain.reasons.join(' ')).toMatch(/whole-word match/);
    expect(regexy.reasons.join(' ')).not.toMatch(/whole-word match/);
  });
});

describe('isLowSignalQueryPath — anchored, no substring false positives (Fix #1)', () => {
  it('treats real low-signal segments as low-signal', () => {
    expect(isLowSignalQueryPath('/repo/dist/x')).toBe(true);
    expect(isLowSignalQueryPath('/repo/tests/auth')).toBe(true);
    expect(isLowSignalQueryPath('/repo/docs')).toBe(true);
    expect(isLowSignalQueryPath('/repo/__fixtures__')).toBe(true);
  });

  it('does NOT match substrings like latest/ contest/ manifest/', () => {
    expect(isLowSignalQueryPath('/repo/src/latest')).toBe(false);
    expect(isLowSignalQueryPath('/repo/src/contest')).toBe(false);
    expect(isLowSignalQueryPath('/repo/src/manifest')).toBe(false);
    expect(isLowSignalQueryPath('/repo/src/app')).toBe(false);
    expect(isLowSignalQueryPath(undefined)).toBe(false);
  });
});

describe('ranking never gates results (graceful degradation)', () => {
  it('returns files in unsupported languages (generic profile, not dropped)', () => {
    const files = [
      file('/repo/src/app.rb', 3, ['def handler', '  fallback', 'end']), // Ruby: no profile
      file('/repo/src/main.swift', 2, ['func fallback() {}']), // Swift: no profile
      file('/repo/src/known.ts', 1, ['export function fallback() {}']),
    ];
    const r = rankFiles(files, 'relevance', ctx());
    // All three survive ranking; nothing dropped.
    expect(r.files.length).toBe(3);
    expect(r.files.map(f => f.path).sort()).toEqual(
      files.map(f => f.path).sort()
    );
  });

  it('a file that throws during scoring is kept with a neutral score', () => {
    const good = file('/repo/src/a.ts', 1, ['export function fallback() {}']);
    // path:null makes the under-subtree check (file.path.startsWith) throw.
    const broken = {
      path: null as unknown as string,
      matchCount: 1,
      matches: [{ line: 1, value: 'fallback' }],
    } as LocalSearchCodeFile;
    const r = rankFiles(
      [broken, good],
      'relevance',
      ctx({ queryPath: '/repo' })
    );
    // Both returned — the bad file did not drop the good one or crash the batch.
    expect(r.files.length).toBe(2);
    // Good (scored) ranks above the neutral-scored broken file.
    expect(r.files[0]?.path).toBe('/repo/src/a.ts');
  });
});

describe('engine AST kind (Phase 2) is preferred over regex heuristics', () => {
  it('uses engine kind:"declaration" even when the line text looks incidental', () => {
    // Line text has no regex-detectable declaration, but the engine labeled it.
    const astDecl: LocalSearchCodeFile = {
      path: '/repo/src/a.ts',
      matchCount: 1,
      matches: [{ line: 1, value: 'fallback', kind: 'declaration' } as never],
    };
    const plain: LocalSearchCodeFile = {
      path: '/repo/src/b.ts',
      matchCount: 1,
      matches: [{ line: 1, value: 'fallback' }],
    };
    const a = scoreFileWithRarity(astDecl, ctx());
    const b = scoreFileWithRarity(plain, ctx());
    expect(a.score).toBeGreaterThan(b.score);
    expect(a.reasons.join(' ')).toMatch(/AST: declaration/);
  });

  it('engine kind:"comment" is penalized as weak evidence', () => {
    const astComment: LocalSearchCodeFile = {
      path: '/repo/src/a.ts',
      matchCount: 1,
      matches: [
        {
          line: 1,
          value: 'export function fallback()',
          kind: 'comment',
        } as never,
      ],
    };
    const a = scoreFileWithRarity(astComment, ctx());
    // Regex would have scored this as a declaration+export (~12); the engine
    // says it is actually inside a comment, so it must not.
    expect(a.reasons.join(' ')).toMatch(/AST: comment/);
    expect(a.reasons.join(' ')).not.toMatch(/declaration line/);
  });
});

describe('language profiles — high-signal lines per language', () => {
  const cases: Array<[string, string, string]> = [
    ['rust', '/repo/src/lib.rs', 'pub fn handler() {}'],
    ['python', '/repo/app/h.py', 'def handler(self):'],
    ['go', '/repo/h.go', 'func Handler() error {'],
    ['markdown', '/repo/README.md', '## handler'],
    ['yaml', '/repo/ci.yaml', 'handler: build'],
  ];

  it.each(cases)(
    '%s declaration/heading/key beats a bare mention',
    (_lang, path, declLine) => {
      const decl = file(path, 1, [declLine]);
      const mention = file(path.replace(/\.(\w+)$/, '.other.$1'), 1, [
        'some text handler in prose',
      ]);
      const declScore = scoreFileWithRarity(decl, ctx({ keyword: 'handler' }));
      const mentionScore = scoreFileWithRarity(mention, ctx({ keyword: 'handler' }));
      expect(declScore.score).toBeGreaterThan(mentionScore.score);
    }
  );
});
