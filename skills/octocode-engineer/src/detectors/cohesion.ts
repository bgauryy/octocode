import { findImportLine, isLikelyEntrypoint } from './shared.js';
import { canAddFinding } from './shared.js';
import { isTestFile } from '../common/utils.js';

import type { FindingDraft } from './shared.js';
import type {
  DependencyState,
  DependencySummary,
  FileCriticality,
  FileEntry,
  Finding,
  HotFile,
} from '../types/index.js';

export function detectGodModules(
  fileSummaries: FileEntry[],
  dependencyState: DependencyState,
  stmtThreshold: number = 500,
  exportThreshold: number = 20
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    const totalStmts = entry.functions.reduce(
      (s, fn) => s + fn.statementCount,
      0
    );
    const exportCount = (
      dependencyState.declaredExportsByFile.get(entry.file) || []
    ).length;
    const reasons: string[] = [];
    if (totalStmts > stmtThreshold)
      reasons.push(`${totalStmts} statements (threshold: ${stmtThreshold})`);
    if (exportCount > exportThreshold)
      reasons.push(`${exportCount} exports (threshold: ${exportThreshold})`);
    if (reasons.length === 0) continue;

    if (!canAddFinding(findings)) break;
    findings.push({
      severity: 'high',
      category: 'god-module',
      file: entry.file,
      lineStart: 1,
      lineEnd: 1,
      title: `God module: ${entry.file}`,
      reason: `Module is excessively large: ${reasons.join('; ')}.`,
      files: [entry.file],
      suggestedFix: {
        strategy:
          'Split module into focused sub-modules with single responsibilities.',
        steps: [
          'Identify distinct functional groups within the module.',
          'Extract each group into a dedicated module.',
          'Create a barrel if backward compatibility is needed.',
          'Update imports incrementally.',
        ],
      },
      impact: 'Smaller modules are easier to understand, test, and maintain.',
      tags: ['complexity', 'responsibility', 'size'],
      lspHints: [
        {
          tool: 'lspGetSemantics', semanticType: 'references',
          symbolName: entry.file.split('/').pop() || entry.file,
          lineHint: 1,
          file: entry.file,
          expectedResult: `identify consumer clusters to guide module splitting strategy`,
        },
      ],
    });
  }

  return findings;
}

function folderOf(filePath: string): string {
  const normalized = filePath.replace(/\\/g, '/');
  const idx = normalized.lastIndexOf('/');
  return idx === -1 ? '.' : normalized.slice(0, idx);
}

export function detectMegaFolders(
  fileSummaries: FileEntry[],
  minFiles: number = 25,
  concentrationThreshold: number = 0.25
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  const productionFiles = fileSummaries.filter(
    entry => !isTestFile(entry.file)
  );
  if (productionFiles.length === 0) return findings;

  const byFolder = new Map<string, FileEntry[]>();
  for (const entry of productionFiles) {
    const folder = folderOf(entry.file);
    if (!byFolder.has(folder)) byFolder.set(folder, []);
    byFolder.get(folder)!.push(entry);
  }

  const sortedFolders = [...byFolder.entries()]
    .map(([folder, entries]) => ({ folder, entries, count: entries.length }))
    .filter(
      ({ count }) =>
        count >= minFiles &&
        count / productionFiles.length >= concentrationThreshold
    )
    .sort((a, b) => b.count - a.count);

  for (const candidate of sortedFolders) {
    const concentration = candidate.count / productionFiles.length;
    const severity: Finding['severity'] =
      concentration >= 0.5 || candidate.count >= 50 ? 'high' : 'medium';
    const topFiles = candidate.entries
      .map(entry => entry.file)
      .sort()
      .slice(0, 8);
    const representativeFile = candidate.entries[0]?.file ?? candidate.folder;

    if (!canAddFinding(findings)) break;
    findings.push({
      severity,
      category: 'mega-folder',
      file: representativeFile,
      lineStart: 1,
      lineEnd: 1,
      title: `Mega folder: ${candidate.folder} (${candidate.count} files)`,
      reason: `${candidate.folder} contains ${candidate.count} production files (${(concentration * 100).toFixed(1)}% of the codebase), which usually indicates mixed responsibilities and weak module boundaries.`,
      files: topFiles,
      suggestedFix: {
        strategy:
          'Map the import graph, identify domain clusters, then restructure with an automated migration script.',
        steps: [
          'Extract the local import graph (rg/localSearchCode) and group files into clusters by what imports what.',
          'Design target directories that follow the data flow (e.g., types → parsing → analysis → detection → reporting → orchestration).',
          'Write a disposable migration script that maps old basenames to { dir, name } targets, moves files, and rewrites all relative import paths atomically.',
          'Validate after each phase: tsc --noEmit, eslint --fix, test suite.',
          'Move shared primitives into a dedicated common/ folder to avoid cross-domain coupling.',
        ],
      },
      impact:
        'Improves navigability, ownership boundaries, and change isolation.',
      tags: [
        'architecture',
        'modularity',
        'folder-structure',
        'maintainability',
      ],
      evidence: {
        folderPath: candidate.folder,
        fileCount: candidate.count,
        totalProductionFiles: productionFiles.length,
        concentration,
      },
      lspHints: [
        {
          tool: 'lspGetSemantics', semanticType: 'definition',
          symbolName: candidate.folder,
          lineHint: 1,
          file: representativeFile,
          expectedResult:
            'inventory representative modules in this folder before planning decomposition',
        },
      ],
    });
  }

  return findings;
}

export function detectGodFunctions(
  fileSummaries: FileEntry[],
  stmtThreshold: number = 100,
  miThreshold: number = 10
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  const MIN_LOC_FOR_MI = 30;

  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    for (const fn of entry.functions) {
      const byStatements = fn.statementCount > stmtThreshold;
      const byMI =
        fn.maintainabilityIndex !== undefined &&
        fn.maintainabilityIndex < miThreshold &&
        fn.lengthLines > MIN_LOC_FOR_MI;

      if (byStatements || byMI) {
        const miNote =
          byMI && fn.maintainabilityIndex !== undefined
            ? ` MI=${fn.maintainabilityIndex.toFixed(1)} (threshold: ${miThreshold}).`
            : '';
        const stmtNote = byStatements
          ? `${fn.statementCount} statements (threshold: ${stmtThreshold}).`
          : '';
        findings.push({
          severity: 'high',
          category: 'god-function',
          file: entry.file,
          lineStart: fn.lineStart,
          lineEnd: fn.lineEnd,
          title: `God function: ${fn.name}`,
          reason: `Function "${fn.name}" triggers god-function detection. ${stmtNote}${miNote}`.trim(),
          files: [`${entry.file}:${fn.lineStart}-${fn.lineEnd}`],
          suggestedFix: {
            strategy: 'Break down into smaller, focused functions.',
            steps: [
              'Identify logical steps within the function.',
              'Extract each step into a named helper.',
              'Keep the original as a high-level orchestrator.',
              'Test each extracted function independently.',
            ],
          },
          impact: 'Improves readability, testability, and maintenance.',
          tags: ['complexity', 'responsibility', 'size'],
          lspHints: [
            {
              tool: 'lspGetSemantics', semanticType: 'callers',
              symbolName: fn.name,
              lineHint: fn.lineStart,
              file: entry.file,
              expectedResult: `map callers and callees to identify safe extraction boundaries for ${fn.name}`,
            },
          ],
        });
      }
    }
  }

  return findings;
}

export function detectLowCohesion(
  dependencyState: DependencyState,
  minExports: number = 3
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  for (const file of dependencyState.files) {
    if (isTestFile(file) || isLikelyEntrypoint(file)) continue;

    const exports = dependencyState.declaredExportsByFile.get(file);
    if (!exports || exports.length < minExports) continue;

    const exportNames = new Set(exports.map(e => e.name));

    const symbolConsumers = new Map<string, Set<string>>();
    for (const [
      consumer,
      imports,
    ] of dependencyState.importedSymbolsByFile.entries()) {
      for (const imp of imports) {
        if (imp.resolvedModule !== file) continue;
        if (!exportNames.has(imp.importedName)) continue;
        if (!symbolConsumers.has(imp.importedName))
          symbolConsumers.set(imp.importedName, new Set());
        symbolConsumers.get(imp.importedName)!.add(consumer);
      }
    }

    const consumedSymbols = [...symbolConsumers.keys()];
    if (consumedSymbols.length < 2) continue;

    const adj = new Map<string, Set<string>>();
    for (const sym of consumedSymbols) adj.set(sym, new Set());

    for (const imports of dependencyState.importedSymbolsByFile.values()) {
      const fromThisFile = imports
        .filter(
          i => i.resolvedModule === file && exportNames.has(i.importedName)
        )
        .map(i => i.importedName);
      for (let i = 0; i < fromThisFile.length; i++) {
        for (let j = i + 1; j < fromThisFile.length; j++) {
          adj.get(fromThisFile[i])?.add(fromThisFile[j]);
          adj.get(fromThisFile[j])?.add(fromThisFile[i]);
        }
      }
    }

    const visited = new Set<string>();
    let components = 0;
    for (const sym of consumedSymbols) {
      if (visited.has(sym)) continue;
      components++;
      const queue = [sym];
      while (queue.length > 0) {
        const curr = queue.pop()!;
        if (visited.has(curr)) continue;
        visited.add(curr);
        for (const neighbor of adj.get(curr) || []) {
          if (!visited.has(neighbor)) queue.push(neighbor);
        }
      }
    }

    if (components > 1) {
      findings.push({
        severity: components >= 4 ? 'high' : 'medium',
        category: 'low-cohesion',
        file,
        lineStart: 1,
        lineEnd: 1,
        title: `Low cohesion: ${file} (LCOM=${components})`,
        reason: `Module exports ${consumedSymbols.length} consumed symbols that form ${components} independent groups. Consumers never import symbols across groups — the module serves unrelated purposes.`,
        files: [file],
        suggestedFix: {
          strategy: `Split into ${components} focused modules, one per cohesion group.`,
          steps: [
            'Identify which exports belong to each independent group.',
            'Create a new module for each group with a descriptive name.',
            'Move exports and their dependencies to the appropriate module.',
            'Update consumer imports to point to the new modules.',
          ],
        },
        impact:
          'Higher cohesion = easier navigation, focused testing, and smaller change blast radius.',
        tags: ['cohesion', 'responsibility', 'architecture'],
      });
    }
  }

  return findings;
}

export function computeHotFiles(
  dependencyState: DependencyState,
  dependencySummary: DependencySummary,
  fileCriticalityByPath: Map<string, FileCriticality>,
  maxResults: number = 20
): HotFile[] {
  const cycleFiles = new Set<string>();
  for (const cycle of dependencySummary.cycles) {
    for (const node of cycle.path) cycleFiles.add(node);
  }

  const criticalPathFiles = new Set<string>();
  for (const cp of dependencySummary.criticalPaths) {
    for (const node of cp.path) criticalPathFiles.add(node);
  }

  const results: HotFile[] = [];
  for (const file of dependencyState.files) {
    if (isTestFile(file)) continue;

    const fanIn = (dependencyState.incoming.get(file) || new Set()).size;
    const fanOut = (dependencyState.outgoing.get(file) || new Set()).size;
    const crit = fileCriticalityByPath.get(file);
    const complexityScore = crit?.score ?? 0;
    const exportCount = (dependencyState.declaredExportsByFile.get(file) || [])
      .length;
    const inCycle = cycleFiles.has(file);
    const onCriticalPath = criticalPathFiles.has(file);

    const riskScore = Math.round(
      fanIn * 3 +
        complexityScore * 0.5 +
        exportCount * 1.5 +
        fanOut * 0.5 +
        (inCycle ? 20 : 0) +
        (onCriticalPath ? 10 : 0)
    );

    if (riskScore > 0) {
      results.push({
        file,
        riskScore,
        fanIn,
        fanOut,
        complexityScore,
        exportCount,
        inCycle,
        onCriticalPath,
      });
    }
  }

  results.sort((a, b) => b.riskScore - a.riskScore);
  return results.slice(0, maxResults);
}

export function detectUntestedCriticalCode(
  dependencyState: DependencyState,
  hotFiles: HotFile[],
  fileCriticalityByPath: Map<string, FileCriticality>,
  criticalityScoreThreshold: number = 40
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  const seen = new Set<string>();

  const hasTestCoverage = (file: string): boolean => {
    const testImporters = dependencyState.incomingFromTests.get(file);
    return !!testImporters && testImporters.size > 0;
  };

  const addFinding = (
    file: string,
    riskScore: number,
    reasons: string[]
  ): void => {
    if (seen.has(file)) return;
    seen.add(file);
    if (isTestFile(file)) return;
    if (hasTestCoverage(file)) return;

    const isCritical = riskScore >= 60;
    if (!canAddFinding(findings)) return;
    findings.push({
      severity: isCritical ? 'critical' : 'high',
      category: 'untested-critical-code',
      file,
      lineStart: 1,
      lineEnd: 1,
      title: `Untested critical code: ${file}`,
      reason: `High-risk file has no test imports. ${reasons.join('; ')} (risk score: ${riskScore}).`,
      files: [file],
      suggestedFix: {
        strategy: 'Add test coverage for this critical module.',
        steps: [
          'Create a test file that imports and exercises the public API of this module.',
          'Focus on the highest-complexity functions and exported behaviors first.',
          'Add integration tests if this module sits on a critical dependency path.',
          'Consider property-based tests for complex data transformations.',
        ],
      },
      impact:
        'Untested critical code is the highest-risk area for regressions and undetected bugs.',
      tags: ['testing', 'coverage', 'change-risk', 'critical'],
    });
  };

  for (const hf of hotFiles) {
    const reasons: string[] = [];
    reasons.push(
      `fan-in=${hf.fanIn}, fan-out=${hf.fanOut}, complexity=${hf.complexityScore}`
    );
    if (hf.inCycle) reasons.push('in dependency cycle');
    if (hf.onCriticalPath) reasons.push('on critical dependency path');
    addFinding(hf.file, hf.riskScore, reasons);
  }

  for (const [file, crit] of fileCriticalityByPath) {
    if (crit.score < criticalityScoreThreshold) continue;
    const reasons = [
      `high complexity score (${crit.score}), ${crit.highComplexityFunctions} high-complexity functions`,
    ];
    addFinding(file, crit.score, reasons);
  }

  findings.sort((a, b) => {
    const sevOrder: Record<string, number> = {
      critical: 4,
      high: 3,
      medium: 2,
      low: 1,
      info: 0,
    };
    return (sevOrder[b.severity] || 0) - (sevOrder[a.severity] || 0);
  });

  return findings.slice(0, 25);
}

export function detectFeatureEnvy(
  dependencyState: DependencyState,
  envyRatio: number = 0.6,
  minSymbols: number = 5
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  for (const [
    file,
    imports,
  ] of dependencyState.importedSymbolsByFile.entries()) {
    if (isTestFile(file)) continue;
    if (!dependencyState.files.has(file)) continue;

    const internalImports = imports.filter(
      i => i.resolvedModule && !i.isTypeOnly
    );
    if (internalImports.length < minSymbols) continue;

    const countByTarget = new Map<string, number>();
    for (const imp of internalImports) {
      if (!imp.resolvedModule) continue;
      countByTarget.set(
        imp.resolvedModule,
        (countByTarget.get(imp.resolvedModule) || 0) + 1
      );
    }

    for (const [target, count] of countByTarget) {
      const ratio = count / internalImports.length;
      if (ratio >= envyRatio && count >= minSymbols) {
        const importRef = findImportLine(dependencyState, file, target);
        findings.push({
          severity: ratio > 0.8 ? 'high' : 'medium',
          category: 'feature-envy',
          file,
          lineStart: importRef.lineStart,
          lineEnd: importRef.lineEnd,
          title: `Feature envy: ${file} → ${target}`,
          reason: `Module imports ${count}/${internalImports.length} symbols (${(ratio * 100).toFixed(0)}%) from "${target}". This suggests the logic may belong in or closer to the target module.`,
          files: [file, target],
          suggestedFix: {
            strategy:
              'Move dependent logic to the target module or extract a shared module.',
            steps: [
              'Identify which functions/logic in this file use the imported symbols.',
              'Move that logic to the target module if it belongs there.',
              'If shared, extract a dedicated module that both can import from.',
              'Reduce the import surface by passing data instead of importing behaviors.',
            ],
          },
          impact:
            'Misplaced logic increases coupling and makes changes ripple across module boundaries.',
          tags: ['coupling', 'responsibility', 'misplaced-logic'],
          lspHints: [
            {
              tool: 'lspGetSemantics', semanticType: 'callers',
              symbolName: file.split('/').pop() || file,
              lineHint: importRef.lineStart,
              file,
              expectedResult: `trace which functions use imports from ${target} to decide what to move`,
            },
            {
              tool: 'lspGetSemantics', semanticType: 'definition',
              symbolName: target.split('/').pop() || target,
              lineHint: importRef.lineStart,
              file,
              expectedResult: `inspect target module to evaluate if logic belongs there`,
            },
          ],
        });
      }
    }
  }

  return findings;
}
