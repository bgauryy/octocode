import { describe, expect, it } from 'vitest';

import {
  categoryBreakdown,
  collectTagCloud,
  computeFeatureScores,
  computeHealthScore,
  computeQualityAspectRatings,
  diverseTopRecommendations,
  diversifyFindings,
  formatFileSize,
  generateSummaryMd,
  severityBreakdown,
} from './summary-md.js';
import {
  ARCHITECTURE_CATEGORIES,
  CODE_QUALITY_CATEGORIES,
  DEAD_CODE_CATEGORIES,
  REPORT_SCHEMA_VERSION,
  SECURITY_CATEGORIES,
  TEST_QUALITY_CATEGORIES,
} from './writer.js';

import type { FullReport } from './writer.js';
import type { FileEntry, Finding } from '../types/index.js';

function makeFinding(overrides: Partial<Finding> = {}): Finding {
  return {
    id: 'test-1',
    severity: 'medium',
    category: 'code-quality',
    title: 'Test finding',
    file: 'src/test.ts',
    lineStart: 1,
    lineEnd: 5,
    reason: 'test reason',
    files: ['src/test.ts'],
    suggestedFix: { strategy: 'test', steps: [] },
    ...overrides,
  } as Finding;
}

describe('report-writer constants', () => {
  it('REPORT_SCHEMA_VERSION is 1.1.0', () => {
    expect(REPORT_SCHEMA_VERSION).toBe('1.1.0');
  });

  it('ARCHITECTURE_CATEGORIES is a Set', () => {
    expect(ARCHITECTURE_CATEGORIES).toBeInstanceOf(Set);
  });

  it('CODE_QUALITY_CATEGORIES is a Set', () => {
    expect(CODE_QUALITY_CATEGORIES).toBeInstanceOf(Set);
  });

  it('DEAD_CODE_CATEGORIES is a Set', () => {
    expect(DEAD_CODE_CATEGORIES).toBeInstanceOf(Set);
  });

  it('SECURITY_CATEGORIES is a Set', () => {
    expect(SECURITY_CATEGORIES).toBeInstanceOf(Set);
  });

  it('TEST_QUALITY_CATEGORIES is a Set', () => {
    expect(TEST_QUALITY_CATEGORIES).toBeInstanceOf(Set);
  });
});

describe('severityBreakdown', () => {
  it('returns zero counts for empty findings', () => {
    const result = severityBreakdown([]);
    expect(result).toEqual({
      critical: 0,
      high: 0,
      medium: 0,
      low: 0,
      info: 0,
    });
  });

  it('counts single severity correctly', () => {
    const findings = [makeFinding({ severity: 'critical' })];
    const result = severityBreakdown(findings);
    expect(result.critical).toBe(1);
    expect(result.high).toBe(0);
  });

  it('counts multiple severities correctly', () => {
    const findings = [
      makeFinding({ id: '1', severity: 'high' }),
      makeFinding({ id: '2', severity: 'high' }),
      makeFinding({ id: '3', severity: 'medium' }),
      makeFinding({ id: '4', severity: 'critical' }),
    ];
    const result = severityBreakdown(findings);
    expect(result.critical).toBe(1);
    expect(result.high).toBe(2);
    expect(result.medium).toBe(1);
    expect(result.low).toBe(0);
    expect(result.info).toBe(0);
  });

  it('counts all five severities', () => {
    const findings = [
      makeFinding({ id: '1', severity: 'critical' }),
      makeFinding({ id: '2', severity: 'high' }),
      makeFinding({ id: '3', severity: 'medium' }),
      makeFinding({ id: '4', severity: 'low' }),
      makeFinding({ id: '5', severity: 'info' }),
    ];
    const result = severityBreakdown(findings);
    expect(result).toEqual({
      critical: 1,
      high: 1,
      medium: 1,
      low: 1,
      info: 1,
    });
  });
});

describe('categoryBreakdown', () => {
  it('returns empty object for empty findings', () => {
    expect(categoryBreakdown([])).toEqual({});
  });

  it('counts single category correctly', () => {
    const findings = [makeFinding({ category: 'dead-export' })];
    const result = categoryBreakdown(findings);
    expect(result['dead-export']).toBe(1);
  });

  it('counts multiple categories correctly', () => {
    const findings = [
      makeFinding({ id: '1', category: 'dead-export' }),
      makeFinding({ id: '2', category: 'dead-export' }),
      makeFinding({ id: '3', category: 'dependency-cycle' }),
    ];
    const result = categoryBreakdown(findings);
    expect(result['dead-export']).toBe(2);
    expect(result['dependency-cycle']).toBe(1);
  });

  it('handles unknown categories', () => {
    const findings = [makeFinding({ category: 'custom-category' })];
    const result = categoryBreakdown(findings);
    expect(result['custom-category']).toBe(1);
  });
});

describe('computeHealthScore', () => {
  it('returns 100 for 0 totalFiles', () => {
    expect(computeHealthScore([], 0)).toBe(100);
    expect(computeHealthScore([makeFinding()], 0)).toBe(100);
  });

  it('returns 100 for no findings', () => {
    expect(computeHealthScore([], 50)).toBe(100);
  });

  it('penalizes critical findings (weight 25)', () => {
    const findings = [makeFinding({ severity: 'critical' })];
    const score = computeHealthScore(findings, 10);
    expect(score).toBe(80);
  });

  it('penalizes high findings (weight 10)', () => {
    const findings = [makeFinding({ severity: 'high' })];
    const score = computeHealthScore(findings, 10);
    expect(score).toBe(91);
  });

  it('penalizes medium findings (weight 3)', () => {
    const findings = [makeFinding({ severity: 'medium' })];
    const score = computeHealthScore(findings, 10);
    expect(score).toBe(97);
  });

  it('penalizes low findings (weight 1)', () => {
    const findings = [makeFinding({ severity: 'low' })];
    const score = computeHealthScore(findings, 10);
    expect(score).toBe(99);
  });

  it('info severity has weight 0', () => {
    const findings = [makeFinding({ severity: 'info' })];
    const score = computeHealthScore(findings, 10);
    expect(score).toBe(100);
  });

  it('penalizes proportional to file count', () => {
    const findings = [makeFinding({ severity: 'high' })];
    const smallRepo = computeHealthScore(findings, 5);
    const largeRepo = computeHealthScore(findings, 100);
    expect(largeRepo).toBeGreaterThan(smallRepo);
  });

  it('floors at 0', () => {
    const findings = Array.from({ length: 10 }, () =>
      makeFinding({ id: `f-${Math.random()}`, severity: 'critical' })
    );
    expect(computeHealthScore(findings, 1)).toBe(4);
  });

  it('rounds score to integer', () => {
    const findings = [makeFinding({ severity: 'medium' })];
    const score = computeHealthScore(findings, 3);
    expect(Number.isInteger(score)).toBe(true);
  });

  it('never returns a perfect score when non-info findings exist in huge repos', () => {
    const high = computeHealthScore([makeFinding({ severity: 'high' })], 10_000);
    const medium = computeHealthScore(
      [makeFinding({ severity: 'medium' })],
      10_000
    );
    const low = computeHealthScore([makeFinding({ severity: 'low' })], 10_000);
    expect(high).toBeLessThan(100);
    expect(medium).toBeLessThan(100);
    expect(low).toBeLessThan(100);
  });
});

describe('collectTagCloud', () => {
  it('returns empty for no findings', () => {
    expect(collectTagCloud([])).toEqual([]);
  });

  it('returns empty when findings have no tags', () => {
    expect(collectTagCloud([makeFinding()])).toEqual([]);
  });

  it('aggregates tags from single finding', () => {
    const findings = [makeFinding({ tags: ['coupling', 'architecture'] })];
    const cloud = collectTagCloud(findings);
    expect(cloud).toHaveLength(2);
    expect(cloud).toContainEqual({ tag: 'coupling', count: 1 });
    expect(cloud).toContainEqual({ tag: 'architecture', count: 1 });
  });

  it('counts duplicate tags across findings', () => {
    const findings = [
      makeFinding({ id: '1', tags: ['coupling', 'architecture'] }),
      makeFinding({ id: '2', tags: ['coupling', 'change-risk'] }),
      makeFinding({ id: '3', tags: ['dead-code'] }),
    ];
    const cloud = collectTagCloud(findings);
    expect(cloud[0]).toEqual({ tag: 'coupling', count: 2 });
    expect(cloud.length).toBe(4);
  });

  it('sorts by count descending', () => {
    const findings = [
      makeFinding({ id: '1', tags: ['a', 'b', 'c'] }),
      makeFinding({ id: '2', tags: ['a', 'b'] }),
      makeFinding({ id: '3', tags: ['a'] }),
    ];
    const cloud = collectTagCloud(findings);
    expect(cloud[0]).toEqual({ tag: 'a', count: 3 });
    expect(cloud[1]).toEqual({ tag: 'b', count: 2 });
    expect(cloud[2]).toEqual({ tag: 'c', count: 1 });
  });

  it('skips findings with undefined tags', () => {
    const findings = [
      makeFinding({ id: '1', tags: ['valid'] }),
      makeFinding({ id: '2' }),
    ];
    const cloud = collectTagCloud(findings);
    expect(cloud).toEqual([{ tag: 'valid', count: 1 }]);
  });
});

describe('computeFeatureScores', () => {
  it('scores active categories and maps them to pillars', () => {
    const rows = computeFeatureScores(
      [
        makeFinding({
          id: 'sec',
          category: 'hardcoded-secret',
          severity: 'critical',
          file: 'src/security.ts',
        }),
        makeFinding({
          id: 'dead',
          category: 'dead-export',
          severity: 'low',
          file: 'src/dead.ts',
        }),
      ],
      100,
      null
    );
    const security = rows.find(r => r.category === 'hardcoded-secret');
    const dead = rows.find(r => r.category === 'dead-export');
    expect(security).toBeDefined();
    expect(security!.pillar).toBe('security');
    expect(security!.score).toBeLessThan(100);
    expect(dead).toBeDefined();
    expect(dead!.pillar).toBe('dead-code');
  });

  it('respects active feature filters', () => {
    const rows = computeFeatureScores(
      [
        makeFinding({
          id: 'only',
          category: 'dead-export',
          severity: 'medium',
          file: 'src/dead.ts',
        }),
      ],
      10,
      new Set(['dead-export'])
    );
    expect(rows).toHaveLength(1);
    expect(rows[0].category).toBe('dead-export');
  });

  it('applies hotspot context penalty for findings in high-risk files', () => {
    const baseRows = computeFeatureScores(
      [
        makeFinding({
          id: 'sec-hot',
          category: 'hardcoded-secret',
          severity: 'high',
          file: 'src/hot.ts',
        }),
      ],
      100,
      null
    );
    const contextRows = computeFeatureScores(
      [
        makeFinding({
          id: 'sec-hot',
          category: 'hardcoded-secret',
          severity: 'high',
          file: 'src/hot.ts',
        }),
      ],
      100,
      null,
      {
        hotFiles: [
          {
            file: 'src/hot.ts',
            riskScore: 90,
            fanIn: 10,
            fanOut: 8,
            complexityScore: 30,
            exportCount: 5,
            inCycle: true,
            onCriticalPath: true,
          },
        ],
      }
    );
    expect(contextRows[0].score).toBeLessThan(baseRows[0].score);
    expect(contextRows[0].contextPenalty).toBeGreaterThan(0);
    expect(contextRows[0].hotspotHits).toBe(1);
  });

  it('does not penalize categories without hotspot overlap', () => {
    const rows = computeFeatureScores(
      [
        makeFinding({
          id: 'sec-cold',
          category: 'hardcoded-secret',
          severity: 'high',
          file: 'src/cold.ts',
        }),
      ],
      100,
      null,
      {
        hotFiles: [
          {
            file: 'src/hot.ts',
            riskScore: 95,
            fanIn: 11,
            fanOut: 9,
            complexityScore: 28,
            exportCount: 6,
            inCycle: true,
            onCriticalPath: false,
          },
        ],
      }
    );
    expect(rows[0].contextPenalty).toBe(0);
    expect(rows[0].hotspotHits).toBe(0);
  });
});

describe('computeQualityAspectRatings', () => {
  function makeFileEntry(overrides: Partial<FileEntry> = {}): FileEntry {
    return {
      package: 'pkg',
      file: 'src/service/user-service.ts',
      parseEngine: 'typescript',
      nodeCount: 10,
      kindCounts: {},
      functions: [
        {
          kind: 'function',
          name: 'loadUserProfile',
          nameHint: 'loadUserProfile',
          file: 'src/service/user-service.ts',
          lineStart: 1,
          lineEnd: 10,
          columnStart: 1,
          columnEnd: 1,
          statementCount: 8,
          complexity: 5,
          maxBranchDepth: 2,
          maxLoopDepth: 1,
          returns: 1,
          awaits: 1,
          calls: 3,
          loops: 0,
          lengthLines: 10,
          cognitiveComplexity: 6,
        },
      ],
      flows: [],
      dependencyProfile: {
        internalDependencies: [],
        externalDependencies: [],
        unresolvedDependencies: [],
        declaredExports: [],
        importedSymbols: [],
        reExports: [],
      },
      ...overrides,
    };
  }

  it('returns weighted hybrid ratings with expected aspects', () => {
    const result = computeQualityAspectRatings(
      [
        makeFinding({
          id: 'a1',
          category: 'dependency-cycle',
          severity: 'high',
          file: 'src/service/user-service.ts',
        }),
        makeFinding({
          id: 'a2',
          category: 'dead-export',
          severity: 'medium',
          file: 'src/common/shared-utils.ts',
        }),
      ],
      {
        fileInventory: [
          makeFileEntry(),
          makeFileEntry({
            file: 'src/common/shared-utils.ts',
            functions: [],
            symbolUsageSummary: {
              declaredExportCount: 12,
              importedSymbolCount: 2,
              internalImportCount: 1,
              externalImportCount: 0,
              reExportCount: 0,
              dominantInternalDependency: null,
            },
          }),
        ],
        hotFiles: [
          {
            file: 'src/service/user-service.ts',
            riskScore: 88,
            fanIn: 7,
            fanOut: 4,
            complexityScore: 20,
            exportCount: 2,
            inCycle: true,
            onCriticalPath: true,
          },
        ],
        reportAnalysis: {
          graphSignals: [],
          astSignals: [],
          combinedSignals: [],
          strongestGraphSignal: {
            kind: 'dependency-hotspot',
            lens: 'graph',
            title: 'Hotspot',
            summary: 'Hotspot found',
            confidence: 'high',
            score: 90,
            files: ['src/service/user-service.ts'],
            categories: ['dependency-cycle'],
            evidence: {},
          },
          strongestAstSignal: null,
          combinedInterpretation: null,
          recommendedValidation: null,
          investigationPrompts: [],
        },
      }
    );
    expect(result.model).toBe('hybrid-ai-structure-v1');
    expect(result.overallScore).toBeGreaterThanOrEqual(0);
    expect(result.overallScore).toBeLessThanOrEqual(100);
    expect(result.aspects.map(a => a.aspect)).toEqual([
      'architecture-structure',
      'folder-topology',
      'naming-quality',
      'common-layer-health',
      'maintainability-evolvability',
      'codebase-consistency',
    ]);
  });

  it('keeps common-layer score neutral-positive when no shared/common folders exist', () => {
    const result = computeQualityAspectRatings(
      [makeFinding({ id: 'n1', category: 'cognitive-complexity' })],
      {
        fileInventory: [
          makeFileEntry({ file: 'src/domain/user/profile-service.ts' }),
        ],
      }
    );
    const commonLayer = result.aspects.find(
      aspect => aspect.aspect === 'common-layer-health'
    );
    expect(commonLayer).toBeDefined();
    expect(commonLayer!.score).toBeGreaterThanOrEqual(80);
  });

  it('ignores test-file findings when includeTests=false', () => {
    const findings: Finding[] = [
      makeFinding({
        id: 'prod-arch',
        category: 'dependency-cycle',
        severity: 'high',
        file: 'src/service/core.ts',
        files: ['src/service/core.ts'],
      }),
      ...Array.from({ length: 12 }, (_, index) =>
        makeFinding({
          id: `test-only-${index}`,
          category: 'dependency-critical-path',
          severity: 'critical',
          file: `tests/service/core-${index}.test.ts`,
          files: [`tests/service/core-${index}.test.ts`],
        })
      ),
    ];

    const baseContext = {
      fileInventory: [
        ...Array.from({ length: 20 }, (_, index) =>
          makeFileEntry({ file: `src/service/core-${index}.ts` })
        ),
        makeFileEntry({ file: 'tests/service/core.test.ts' }),
      ],
    };
    const withoutTests = computeQualityAspectRatings(findings, {
      ...baseContext,
      includeTests: false,
    });
    const withTests = computeQualityAspectRatings(findings, {
      ...baseContext,
      includeTests: true,
    });
    const withoutTestsScore = withoutTests.aspects.find(
      aspect => aspect.aspect === 'architecture-structure'
    )!.score;
    const withTestsScore = withTests.aspects.find(
      aspect => aspect.aspect === 'architecture-structure'
    )!.score;

    expect(withoutTestsScore).toBeGreaterThan(withTestsScore);
  });

  it('downweights advisory dead-code categories in maintainability scoring', () => {
    const advisoryFindings = Array.from({ length: 20 }, (_, index) =>
      makeFinding({
        id: `advisory-${index}`,
        category: 'move-to-caller',
        severity: 'low',
        file: `src/tools/advisory-${index}.ts`,
        files: [`src/tools/advisory-${index}.ts`],
      })
    );
    const concreteFindings = Array.from({ length: 20 }, (_, index) =>
      makeFinding({
        id: `concrete-${index}`,
        category: 'dead-file',
        severity: 'low',
        file: `src/tools/concrete-${index}.ts`,
        files: [`src/tools/concrete-${index}.ts`],
      })
    );

    const advisoryScore = computeQualityAspectRatings(advisoryFindings, {
      fileInventory: advisoryFindings.map(f => makeFileEntry({ file: f.file })),
    }).aspects.find(a => a.aspect === 'maintainability-evolvability')!.score;
    const concreteScore = computeQualityAspectRatings(concreteFindings, {
      fileInventory: concreteFindings.map(f => makeFileEntry({ file: f.file })),
    }).aspects.find(a => a.aspect === 'maintainability-evolvability')!.score;

    expect(advisoryScore).toBeGreaterThan(concreteScore);
  });

  it('normalizes folder topology for scans under a shared path prefix', () => {
    const result = computeQualityAspectRatings([], {
      fileInventory: [
        makeFileEntry({
          file: 'packages/octocode-mcp/src/tools/local_find_files/findFiles.ts',
        }),
        makeFileEntry({
          file: 'packages/octocode-mcp/src/tools/local_ripgrep/ripgrepExecutor.ts',
        }),
        makeFileEntry({
          file: 'packages/octocode-mcp/src/tools/local_view_structure/structureWalker.ts',
        }),
      ],
    });

    const folderAspect = result.aspects.find(
      aspect => aspect.aspect === 'folder-topology'
    );
    expect(folderAspect).toBeDefined();
    expect(folderAspect!.score).toBeGreaterThanOrEqual(85);
  });

  it('penalizes leaf folders that accumulate too many files', () => {
    const result = computeQualityAspectRatings([], {
      fileInventory: [
        ...Array.from({ length: 30 }, (_, index) =>
          makeFileEntry({ file: `src/core/module-${index}.ts` })
        ),
        ...Array.from({ length: 5 }, (_, index) =>
          makeFileEntry({ file: `src/api/route-${index}.ts` })
        ),
      ],
    });

    const folderAspect = result.aspects.find(
      aspect => aspect.aspect === 'folder-topology'
    );
    expect(folderAspect).toBeDefined();
    expect(folderAspect!.score).toBeLessThan(85);
    expect(folderAspect!.signals).toContainEqual(
      expect.objectContaining({
        label: 'Bloated folders',
        value: '1/2',
        effect: 'negative',
      })
    );
  });

  it('includes folder naming consistency in codebase consistency', () => {
    const result = computeQualityAspectRatings([], {
      fileInventory: [
        makeFileEntry({ file: 'src/user-profile/loadUser.ts' }),
        makeFileEntry({ file: 'src/user_profile/saveUser.ts' }),
        makeFileEntry({ file: 'src/UserProfile/listUser.ts' }),
      ],
    });

    const consistencyAspect = result.aspects.find(
      aspect => aspect.aspect === 'codebase-consistency'
    );
    expect(consistencyAspect).toBeDefined();
    expect(consistencyAspect!.signals).toContainEqual(
      expect.objectContaining({
        label: 'Dominant folder naming style',
        value: '33%',
        effect: 'negative',
      })
    );
  });

  it('ignores generated and minified paths in hybrid quality rating by default', () => {
    const result = computeQualityAspectRatings(
      [
        makeFinding({
          id: 'gen-1',
          category: 'dependency-cycle',
          severity: 'high',
          file: 'dist/vendor.min.js',
          files: ['dist/vendor.min.js'],
        }),
      ],
      {
        fileInventory: [
          makeFileEntry({ file: 'dist/vendor.min.js', functions: [] }),
          makeFileEntry({ file: 'src/service/core.ts' }),
        ],
      }
    );

    const architectureScore = result.aspects.find(
      aspect => aspect.aspect === 'architecture-structure'
    )!.score;
    expect(architectureScore).toBeGreaterThanOrEqual(90);
  });
});

describe('formatFileSize', () => {
  it('formats bytes as B when < 1024', () => {
    expect(formatFileSize(0)).toBe('0 B');
    expect(formatFileSize(512)).toBe('512 B');
    expect(formatFileSize(1023)).toBe('1023 B');
  });

  it('formats as KB when >= 1024 and < 1024*1024', () => {
    expect(formatFileSize(1024)).toBe('1.0 KB');
    expect(formatFileSize(2048)).toBe('2.0 KB');
    expect(formatFileSize(1536)).toBe('1.5 KB');
  });

  it('formats as MB when >= 1024*1024', () => {
    expect(formatFileSize(1024 * 1024)).toBe('1.0 MB');
    expect(formatFileSize(2.5 * 1024 * 1024)).toBe('2.5 MB');
  });
});

describe('diversifyFindings', () => {
  const makeDraft = (
    severity: Finding['severity'],
    category: string,
    idx: number
  ) =>
    makeFinding({
      id: `draft-${category}-${idx}`,
      severity,
      category,
      file: `${category}-${idx}.ts`,
    });

  it('returns all when limit >= sorted.length', () => {
    const input = [makeDraft('high', 'a', 1), makeDraft('high', 'b', 1)];
    expect(diversifyFindings(input, 10)).toBe(input);
    expect(diversifyFindings(input, 2)).toBe(input);
  });

  it('returns all when limit is Infinity', () => {
    const input = [makeDraft('high', 'a', 1)];
    expect(diversifyFindings(input, Infinity)).toBe(input);
  });

  it('returns diverse set across categories when limit < length', () => {
    const input = [
      ...Array.from({ length: 10 }, (_, i) =>
        makeDraft('high', 'await-in-loop', i)
      ),
      makeDraft('high', 'dead-export', 1),
      makeDraft('high', 'dead-export', 2),
    ];
    const result = diversifyFindings(input, 5);
    expect(result).toHaveLength(5);
    const categories = new Set(result.map(f => f.category));
    expect(categories.size).toBe(2);
  });

  it('prioritizes categories by highest severity', () => {
    const input = [
      makeDraft('critical', 'security', 1),
      makeDraft('high', 'quality', 1),
      makeDraft('medium', 'dead-code', 1),
    ];
    const result = diversifyFindings(input, 3);
    expect(result[0].category).toBe('security');
    expect(result[1].category).toBe('quality');
    expect(result[2].category).toBe('dead-code');
  });

  it('handles empty input', () => {
    expect(diversifyFindings([], 5)).toEqual([]);
  });

  it('handles single category', () => {
    const input = Array.from({ length: 10 }, (_, i) =>
      makeDraft('high', 'only-cat', i)
    );
    const result = diversifyFindings(input, 3);
    expect(result).toHaveLength(3);
    expect(result.every(f => f.category === 'only-cat')).toBe(true);
  });

  it('handles limit of 1', () => {
    const input = [makeDraft('critical', 'a', 1), makeDraft('high', 'b', 1)];
    const result = diversifyFindings(input, 1);
    expect(result).toHaveLength(1);
    expect(result[0].severity).toBe('critical');
  });

  it('returns empty when limit is 0 and input has items', () => {
    const input = [makeDraft('high', 'a', 1)];
    const result = diversifyFindings(input, 0);
    expect(result).toEqual([]);
  });

  it('handles non-finite limit by returning all', () => {
    const input = [makeDraft('high', 'a', 1)];
    expect(diversifyFindings(input, NaN)).toBe(input);
  });
});

describe('diverseTopRecommendations', () => {
  it('limits findings per category (default maxPerCategory=2)', () => {
    const findings = [
      makeFinding({ id: '1', category: 'dead-export' }),
      makeFinding({ id: '2', category: 'dead-export' }),
      makeFinding({ id: '3', category: 'dead-export' }),
      makeFinding({ id: '4', category: 'cognitive-complexity' }),
      makeFinding({ id: '5', category: 'cognitive-complexity' }),
    ];
    const result = diverseTopRecommendations(findings, 10);
    expect(result).toHaveLength(4);
    expect(result.filter(f => f.category === 'dead-export')).toHaveLength(2);
    expect(
      result.filter(f => f.category === 'cognitive-complexity')
    ).toHaveLength(2);
  });

  it('respects total limit', () => {
    const findings = Array.from({ length: 30 }, (_, i) =>
      makeFinding({ id: `${i}`, category: `cat-${i % 10}` })
    );
    const result = diverseTopRecommendations(findings, 5, 2);
    expect(result).toHaveLength(5);
  });

  it('returns empty for empty input', () => {
    expect(diverseTopRecommendations([], 10)).toHaveLength(0);
  });

  it('uses maxPerCategory=1 for maximum diversity', () => {
    const findings = [
      makeFinding({ id: '1', category: 'a' }),
      makeFinding({ id: '2', category: 'a' }),
      makeFinding({ id: '3', category: 'b' }),
      makeFinding({ id: '4', category: 'b' }),
      makeFinding({ id: '5', category: 'c' }),
    ];
    const result = diverseTopRecommendations(findings, 10, 1);
    expect(result).toHaveLength(3);
    expect(new Set(result.map(f => f.category)).size).toBe(3);
  });

  it('returns all when limit exceeds available diverse findings', () => {
    const findings = [
      makeFinding({ id: '1', category: 'a' }),
      makeFinding({ id: '2', category: 'b' }),
    ];
    const result = diverseTopRecommendations(findings, 10, 2);
    expect(result).toHaveLength(2);
  });

  it('preserves input order, taking first maxPerCategory per category', () => {
    const findings = [
      makeFinding({ id: '1', category: 'a', severity: 'critical' }),
      makeFinding({ id: '2', category: 'a', severity: 'high' }),
      makeFinding({ id: '3', category: 'b', severity: 'medium' }),
    ];
    const result = diverseTopRecommendations(findings, 10, 2);
    expect(result[0].id).toBe('1');
    expect(result[1].id).toBe('2');
    expect(result[2].id).toBe('3');
  });
});

function makeMinimalReport(
  overrides: Partial<FullReport> = {}
): FullReport {
  return {
    generatedAt: '2026-01-01T00:00:00.000Z',
    repoRoot: '/test',
    options: {},
    parser: {},
    summary: {
      totalFiles: 10,
      totalFunctions: 50,
      totalFlows: 20,
      totalDependencyFiles: 10,
      totalPackages: 1,
    },
    fileInventory: [],
    duplicateFlows: {},
    dependencyGraph: {
      totalModules: 5,
      totalEdges: 8,
      cycles: [],
      criticalPaths: [],
      rootsCount: 2,
      leavesCount: 1,
      testOnlyModules: [],
      unresolvedEdgeCount: 0,
    } as FullReport['dependencyGraph'],
    dependencyFindings: [],
    agentOutput: {
      totalFindings: 2,
      findingStats: {
        overall: {
          totalFindings: 2,
          severityBreakdown: { critical: 0, high: 1, medium: 1, low: 0, info: 0 },
        },
      },
      topRecommendations: [
        {
          id: 'AST-0001',
          file: 'src/a.ts',
          severity: 'high',
          category: 'cognitive-complexity',
          title: 'High complexity in foo',
          reason: 'too complex',
        },
      ],
    },
    optimizationOpportunities: [],
    optimizationFindings: [
      makeFinding({ severity: 'high', category: 'cognitive-complexity' }),
      makeFinding({ severity: 'medium', category: 'dead-export' }),
    ],
    parseErrors: [],
    ...overrides,
  };
}

describe('generateSummaryMd', () => {
  it('includes report header with generated date and root', () => {
    const md = generateSummaryMd({
      dir: '/tmp/scan',
      report: makeMinimalReport(),
      outputFiles: { summary: 'summary.json' },
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
    });
    expect(md).toContain('# Code Quality Scan Report');
    expect(md).toContain('2026-01-01T00:00:00.000Z');
    expect(md).toContain('`/test`');
  });

  it('includes scan scope metrics from summary', () => {
    const md = generateSummaryMd({
      dir: '/tmp/scan',
      report: makeMinimalReport(),
      outputFiles: { summary: 'summary.json' },
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
    });
    expect(md).toContain('Files analyzed | 10');
    expect(md).toContain('Functions | 50');
    expect(md).toContain('Flow nodes | 20');
  });

  it('includes findings overview severity table', () => {
    const md = generateSummaryMd({
      dir: '/tmp/scan',
      report: makeMinimalReport(),
      outputFiles: { summary: 'summary.json' },
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
    });
    expect(md).toContain('## Findings Overview');
    expect(md).toContain('High | 1');
    expect(md).toContain('**Total** | **2**');
  });

  it('includes health scores section', () => {
    const md = generateSummaryMd({
      dir: '/tmp/scan',
      report: makeMinimalReport(),
      outputFiles: { summary: 'summary.json' },
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
    });
    expect(md).toContain('## Health Scores');
    expect(md).toContain('**Overall**');
  });

  it('includes feature scores section with category rows', () => {
    const md = generateSummaryMd({
      dir: '/tmp/scan',
      report: makeMinimalReport(),
      outputFiles: { summary: 'summary.json' },
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
    });
    expect(md).toContain('## Feature Scores');
    expect(md).toContain('## AI + Structure Ratings');
    expect(md).toContain('`cognitive-complexity`');
    expect(md).toContain('`dead-export`');
  });

  it('includes features filter when activeFeatures is provided', () => {
    const md = generateSummaryMd({
      dir: '/tmp/scan',
      report: makeMinimalReport(),
      outputFiles: { summary: 'summary.json' },
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
      activeFeatures: new Set(['cognitive-complexity', 'dead-export']),
    });
    expect(md).toContain('**Features filter**');
  });

  it('includes scope annotation when scope is provided', () => {
    const md = generateSummaryMd({
      dir: '/tmp/scan',
      report: makeMinimalReport(),
      outputFiles: { summary: 'summary.json' },
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
      scope: ['/test/src/foo'],
      root: '/test',
    });
    expect(md).toContain('**Scoped scan**');
    expect(md).toContain('src/foo');
  });

  it('includes top recommendations from agentOutput', () => {
    const md = generateSummaryMd({
      dir: '/tmp/scan',
      report: makeMinimalReport(),
      outputFiles: { summary: 'summary.json' },
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
    });
    expect(md).toContain('## Top Recommendations');
    expect(md).toContain('High complexity in foo');
  });

  it('includes output files table', () => {
    const md = generateSummaryMd({
      dir: '/tmp/scan',
      report: makeMinimalReport(),
      outputFiles: { summary: 'summary.json', findings: 'findings.json' },
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
    });
    expect(md).toContain('## Output Files');
    expect(md).toContain('summary.json');
    expect(md).toContain('findings.json');
  });

  it('includes agent instructions section', () => {
    const md = generateSummaryMd({
      dir: '/tmp/scan',
      report: makeMinimalReport(),
      outputFiles: { summary: 'summary.json', findings: 'findings.json' },
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
    });
    expect(md).toContain('## Agent Instructions');
    expect(md).toContain('Validate Before Presenting');
    expect(md).toContain('localSearchCode');
    expect(md).toContain('lspGetSemantics');
    expect(md).toContain('lspGetSemantics');
    expect(md).toContain('callers');
    expect(md).toContain('False Positive Checklist');
    expect(md).toContain('ast/search.js');
  });

  it('includes AST triage row when astTrees output exists', () => {
    const md = generateSummaryMd({
      dir: '/tmp/scan',
      report: makeMinimalReport(),
      outputFiles: { summary: 'summary.json', astTrees: 'ast-trees.txt' },
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
    });
    expect(md).toContain('ast/tree-search.js');
    expect(md).toContain('AST triage');
  });

  it('shows high/critical filter when high severity findings exist', () => {
    const report = makeMinimalReport();
    const md = generateSummaryMd({
      dir: '/tmp/scan',
      report,
      outputFiles: { summary: 'summary.json' },
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
    });
    expect(md).toContain('High/critical findings');
  });

  it('includes parse errors section when present', () => {
    const report = makeMinimalReport({
      parseErrors: [{ file: 'bad.ts', message: 'Unexpected token' }],
    });
    const md = generateSummaryMd({
      dir: '/tmp/scan',
      report,
      outputFiles: { summary: 'summary.json' },
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
    });
    expect(md).toContain('## Parse Errors');
    expect(md).toContain('bad.ts');
  });
});
