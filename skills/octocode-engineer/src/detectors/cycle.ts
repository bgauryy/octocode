import { findImportLine, isLikelyEntrypoint } from './shared.js';
import { canAddFinding } from './shared.js';
import { isTestFile } from '../common/utils.js';

import type { FindingDraft } from './shared.js';
import type {
  DependencyState,
  DependencySummary,
} from '../types/index.js';

export function detectTestOnlyModules(
  dependencySummary: DependencySummary
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  if (dependencySummary.testOnlyModules?.length === 0) return findings;
  for (const file of (dependencySummary.testOnlyModules || []).slice(0, 25)) {
    if (!canAddFinding(findings)) break;
    findings.push({
      severity: 'medium',
      category: 'dependency-test-only',
      file: file.file,
      lineStart: file.lineStart || 1,
      lineEnd: file.lineEnd || 1,
      title: `Module imported only from tests: ${file.file}`,
      reason:
        'No production file imports this module, but tests do. Verify if this module belongs in test fixtures/helpers.',
      files: [file.file],
      suggestedFix: {
        strategy:
          'Move test-only utilities to test scope or make production usage explicit.',
        steps: [
          'Re-run import scanning after moving test-only modules to __tests__ or helper folders.',
          'If this is shared production utility, add a non-test entrypoint/import.',
          'Remove dead or stale production references and delete unused module if confirmed.',
        ],
      },
      impact:
        'Reduces shipping of non-production-only modules and clarifies ownership boundaries.',
      tags: ['testing', 'dead-code', 'dependency'],
    });
  }
  return findings;
}

export function detectDependencyCycles(
  dependencySummary: DependencySummary,
  dependencyState: DependencyState
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  if (dependencySummary.cycles?.length === 0) return findings;
  for (const cycle of (dependencySummary.cycles || []).slice(0, 15)) {
    const cycleLine = findImportLine(
      dependencyState,
      cycle.path[0],
      cycle.path[1]
    );
    if (!canAddFinding(findings)) break;
    findings.push({
      severity: 'high',
      category: 'dependency-cycle',
      file: cycle.path[0],
      lineStart: cycleLine.lineStart,
      lineEnd: cycleLine.lineEnd,
      title: `Dependency cycle detected (${cycle.nodeCount} node cycle)`,
      reason: `Import cycle exists across: ${cycle.path.join(' -> ')}`,
      files: cycle.path,
      suggestedFix: {
        strategy:
          'Break the cycle with a lower-level abstraction or interface module.',
        steps: [
          'Extract shared contracts/types to a dedicated contract/shared package.',
          'Move implementation in one direction using dependency inversion.',
          'Split stateful modules into protocol and runtime layers.',
        ],
      },
      impact:
        'Cycles increase coupling and make incremental loading/debugging and refactors riskier.',
      tags: ['cycle', 'coupling', 'dependency', 'change-risk'],
      lspHints: [
        {
          tool: 'lspGetSemantics', semanticType: 'definition',
          symbolName: cycle.path[1],
          lineHint: cycleLine.lineStart,
          file: cycle.path[0],
          expectedResult: `navigate to the import that creates the cycle edge`,
        },
      ],
    });
  }
  return findings;
}

function findChainHotspot(
  chainPath: string[],
  dependencyState: DependencyState
): { module: string; fanOut: number; fanIn: number } {
  let best = { module: chainPath[0], fanOut: 0, fanIn: 0 };
  for (const mod of chainPath) {
    const fanOut = (dependencyState.outgoing.get(mod) || new Set()).size;
    const fanIn = (dependencyState.incoming.get(mod) || new Set()).size;
    if (fanOut > best.fanOut) {
      best = { module: mod, fanOut, fanIn };
    }
  }
  return best;
}

export function mergeOverlappingChains(
  findings: FindingDraft[],
  overlapThreshold: number = 0.8
): FindingDraft[] {
  if (findings.length <= 1) return findings;

  const merged: FindingDraft[] = [];
  const consumed = new Set<number>();

  for (let i = 0; i < findings.length; i++) {
    if (consumed.has(i)) continue;
    const base = findings[i];
    const baseSet = new Set(base.files);
    const entryPoints = [base.file];

    for (let j = i + 1; j < findings.length; j++) {
      if (consumed.has(j)) continue;
      const other = findings[j];
      const otherSet = new Set(other.files);
      const intersection = [...baseSet].filter(f => otherSet.has(f)).length;
      const union = new Set([...baseSet, ...otherSet]).size;
      const overlap = union > 0 ? intersection / union : 0;

      if (overlap >= overlapThreshold) {
        consumed.add(j);
        entryPoints.push(other.file);
        for (const f of other.files) baseSet.add(f);
      }
    }

    if (entryPoints.length > 1) {
      const allFiles = [...baseSet];
      merged.push({
        ...base,
        title: `Critical dependency chain risk: ${allFiles.length} files (${entryPoints.length} entry points)`,
        reason:
          base.reason +
          ` Also reached from: ${entryPoints.slice(1).join(', ')}.`,
        files: allFiles,
      });
    } else {
      merged.push(base);
    }
  }

  return merged;
}

export function detectCriticalPaths(
  dependencySummary: DependencySummary,
  dependencyState: DependencyState,
  criticalComplexityThreshold: number
): FindingDraft[] {
  const rawFindings: FindingDraft[] = [];
  if (dependencySummary.criticalPaths?.length === 0) return rawFindings;
  for (const pathEntry of (dependencySummary.criticalPaths || []).slice(
    0,
    10
  )) {
    if (pathEntry.score < criticalComplexityThreshold * 3) continue;
    const chainLine = findImportLine(
      dependencyState,
      pathEntry.path[0],
      pathEntry.path[1]
    );
    const hotspot = findChainHotspot(pathEntry.path, dependencyState);
    rawFindings.push({
      severity:
        pathEntry.score >= criticalComplexityThreshold * 6
          ? 'critical'
          : 'high',
      category: 'dependency-critical-path',
      file: pathEntry.path[0],
      lineStart: chainLine.lineStart,
      lineEnd: chainLine.lineEnd,
      title: `Critical dependency chain risk: ${pathEntry.length} files`,
      reason: `Potentially high-change surface: ${pathEntry.path.join(' -> ')} (${pathEntry.score} weight).`,
      files: pathEntry.path,
      suggestedFix: {
        strategy: `Break chain at \`${hotspot.module}\` (fan-out: ${hotspot.fanOut}, fan-in: ${hotspot.fanIn}).`,
        steps: [
          `Extract interface from \`${hotspot.module}\` — it has ${hotspot.fanOut} outbound dependencies.`,
          'Downstream modules depend on the interface, not the implementation.',
          'This splits the chain into two independent segments.',
        ],
      },
      impact:
        'Critical refactor opportunities; shorter chains reduce blast radius of change.',
      tags: ['change-risk', 'dependency', 'blast-radius'],
    });
  }
  return mergeOverlappingChains(rawFindings);
}

export function detectDeadFiles(
  dependencySummary: DependencySummary,
  dependencyState: DependencyState
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const file of dependencySummary.roots || []) {
    if (isTestFile(file)) continue;
    if (isLikelyEntrypoint(file)) continue;
    const incomingCount = (dependencyState.incoming.get(file) || new Set())
      .size;
    const outgoingCount = (dependencyState.outgoing.get(file) || new Set())
      .size;
    if (incomingCount !== 0) continue;
    if (outgoingCount > 0) continue;
    if (!canAddFinding(findings)) break;
    findings.push({
      severity: 'medium',
      category: 'dead-file',
      file,
      lineStart: 1,
      lineEnd: 1,
      title: `Potential dead file: ${file}`,
      reason:
        'File has no inbound imports and no outbound dependencies. It may be stale or orphaned.',
      files: [file],
      suggestedFix: {
        strategy: 'Validate ownership and remove if truly unused.',
        steps: [
          'Confirm the file is not an explicit runtime entrypoint.',
          'Search runtime config/router/bootstrap references for this file path.',
          'Delete file if confirmed dead and re-run scan.',
        ],
      },
      impact: 'Reduces dead surface area and maintenance overhead.',
      tags: ['dead-code', 'cleanup', 'hygiene'],
      lspHints: [
        {
          tool: 'lspGetSemantics', semanticType: 'references',
          symbolName: file.split('/').pop() || file,
          lineHint: 1,
          file,
          expectedResult: `confirm zero references exist before deletion`,
        },
      ],
    });
  }
  return findings;
}

export function detectOrphanModules(
  dependencyState: DependencyState
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  for (const file of dependencyState.files) {
    if (isTestFile(file)) continue;
    if (isLikelyEntrypoint(file)) continue;

    const ca = (dependencyState.incoming.get(file) || new Set()).size;
    const ce = (dependencyState.outgoing.get(file) || new Set()).size;

    if (ca === 0 && ce === 0) {
      findings.push({
        severity: 'medium',
        category: 'orphan-module',
        file,
        lineStart: 1,
        lineEnd: 1,
        title: `Orphan module: ${file}`,
        reason:
          'Module has no inbound or outbound dependencies — completely disconnected from the module graph.',
        files: [file],
        suggestedFix: {
          strategy: 'Delete if truly unused, or wire into module graph.',
          steps: [
            'Check if the file is a runtime entrypoint, route, or config.',
            'If truly disconnected, delete and re-run scan.',
            'If needed, add an explicit import from the appropriate parent module.',
          ],
        },
        impact: 'Removes dead surface area and clarifies module ownership.',
        tags: ['dead-code', 'dependency', 'isolation'],
      });
    }
  }

  return findings;
}

export function detectUnreachableModules(
  dependencyState: DependencyState
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  const entrypoints = new Set<string>();
  for (const file of dependencyState.files) {
    if (isLikelyEntrypoint(file)) entrypoints.add(file);
  }
  if (entrypoints.size === 0) {
    for (const file of dependencyState.files) {
      if ((dependencyState.incoming.get(file) || new Set()).size === 0) {
        entrypoints.add(file);
      }
    }
  }

  const reachable = new Set<string>();
  const queue = [...entrypoints];
  while (queue.length > 0) {
    const current = queue.pop()!;
    if (reachable.has(current)) continue;
    reachable.add(current);
    for (const dep of dependencyState.outgoing.get(current) || new Set()) {
      if (dependencyState.files.has(dep) && !reachable.has(dep))
        queue.push(dep);
    }
  }

  for (const file of dependencyState.files) {
    if (isTestFile(file) || reachable.has(file) || isLikelyEntrypoint(file))
      continue;
    if (!canAddFinding(findings)) break;
    findings.push({
      severity: 'high',
      category: 'unreachable-module',
      file,
      lineStart: 1,
      lineEnd: 1,
      title: `Unreachable module: ${file}`,
      reason:
        'Module is not reachable from any entrypoint via the import graph.',
      files: [file],
      suggestedFix: {
        strategy: 'Verify reachability and remove if truly dead.',
        steps: [
          'Check if this module is loaded dynamically or via framework conventions.',
          'Verify it is not registered as a route, plugin, or middleware.',
          'If confirmed unreachable, delete and re-run scan.',
        ],
      },
      impact:
        'Identifies potentially large sections of dead code missed by direct-import checks.',
      tags: ['dead-code', 'dependency', 'reachability'],
    });
  }

  return findings;
}
