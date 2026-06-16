import { isLikelyEntrypoint } from './shared.js';
import { canAddFinding } from './shared.js';
import { isTestFile } from '../common/utils.js';

import type { FindingDraft } from './shared.js';
import type {
  DependencyState,
  DependencySummary,
  FileEntry,
  Finding,
  HotFile,
} from '../types/index.js';

export function computeBarrelDepth(
  file: string,
  dependencyState: DependencyState,
  visited: Set<string> = new Set()
): number {
  if (visited.has(file)) return 0;
  visited.add(file);

  const reexports = dependencyState.reExportsByFile.get(file);
  if (!reexports || reexports.length === 0) return 0;

  let maxChild = 0;
  for (const re of reexports) {
    const target = re.resolvedModule;
    if (!target) continue;
    const targetRe = dependencyState.reExportsByFile.get(target);
    if (targetRe && targetRe.length > 0) {
      maxChild = Math.max(
        maxChild,
        computeBarrelDepth(target, dependencyState, visited)
      );
    }
  }

  return 1 + maxChild;
}

export function detectBarrelExplosion(
  dependencyState: DependencyState,
  symbolThreshold: number = 30
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  for (const [file, reexports] of dependencyState.reExportsByFile.entries()) {
    if (isTestFile(file)) continue;
    if (reexports.length === 0) continue;

    if (reexports.length > symbolThreshold) {
      findings.push({
        severity: reexports.length > symbolThreshold * 2 ? 'high' : 'medium',
        category: 'barrel-explosion',
        file,
        lineStart: 1,
        lineEnd: 1,
        title: `Barrel explosion: ${file}`,
        reason: `Barrel re-exports ${reexports.length} symbols (threshold: ${symbolThreshold}). Large barrels hurt bundling.`,
        files: [file],
        suggestedFix: {
          strategy:
            'Split barrel or use direct imports to reduce bundler cost.',
          steps: [
            'Group re-exports by domain into sub-barrels.',
            'Let consumers import directly from source modules.',
            'Remove unused re-exports (check dead-re-export findings).',
          ],
        },
        impact: 'Reduces bundle size and speeds up IDE/tooling.',
        tags: ['barrel', 'bundle-size', 'tree-shaking'],
      });
    }

    const depth = computeBarrelDepth(file, dependencyState);
    if (depth > 2) {
      findings.push({
        severity: 'high',
        category: 'barrel-explosion',
        file,
        lineStart: 1,
        lineEnd: 1,
        title: `Deep barrel chain: ${file} (depth ${depth})`,
        reason: `Barrel chain is ${depth} levels deep. Deep chains defeat tree-shaking.`,
        files: [file],
        suggestedFix: {
          strategy: 'Flatten barrel chain to at most 2 levels.',
          steps: [
            'Re-export directly from source modules instead of intermediate barrels.',
            'Remove intermediate barrel layers that add no value.',
          ],
        },
        impact: 'Improves tree-shaking efficiency and import resolution speed.',
        tags: ['barrel', 'bundle-size', 'tree-shaking'],
      });
    }
  }

  return findings;
}

export function detectImportSideEffectRisk(
  fileSummaries: FileEntry[],
  dependencyState: DependencyState,
  dependencySummary: DependencySummary,
  hotFiles: HotFile[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  const cycleFiles = new Set<string>();
  for (const cycle of dependencySummary.cycles) {
    for (const node of cycle.path) cycleFiles.add(node);
  }
  const criticalPathFiles = new Set<string>();
  for (const cp of dependencySummary.criticalPaths) {
    for (const node of cp.path) criticalPathFiles.add(node);
  }
  const hotFileMap = new Map<string, HotFile>();
  for (const hf of hotFiles) hotFileMap.set(hf.file, hf);

  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    const effects = entry.topLevelEffects;
    if (!effects || effects.length === 0) continue;

    let astBase = 0;
    for (const eff of effects) astBase += eff.weight;

    const fanIn = (dependencyState.incoming.get(entry.file) || new Set()).size;
    let impactBoost = 0;
    if (fanIn >= 20) impactBoost += 8;
    else if (fanIn >= 8) impactBoost += 4;
    if (criticalPathFiles.has(entry.file)) impactBoost += 6;
    if (cycleFiles.has(entry.file)) impactBoost += 3;

    let roleDiscount = 0;
    if (isLikelyEntrypoint(entry.file)) roleDiscount += 4;

    const totalRisk = astBase + impactBoost - roleDiscount;
    if (totalRisk < 4) continue;

    const severity: Finding['severity'] =
      totalRisk >= 18
        ? 'critical'
        : totalRisk >= 12
          ? 'high'
          : totalRisk >= 7
            ? 'medium'
            : 'low';

    const highConfidenceEffects = effects.filter(e => e.confidence === 'high');
    const confidence: 'high' | 'medium' | 'low' =
      highConfidenceEffects.length > 0
        ? 'high'
        : effects.some(e => e.confidence === 'medium')
          ? 'medium'
          : 'low';

    const effectDetails = effects
      .map(e => `${e.detail} (line ${e.lineStart})`)
      .join('; ');
    const impactDetails: string[] = [];
    if (fanIn >= 8) impactDetails.push(`fan-in=${fanIn}`);
    if (criticalPathFiles.has(entry.file))
      impactDetails.push('on critical path');
    if (cycleFiles.has(entry.file)) impactDetails.push('in dependency cycle');
    if (isLikelyEntrypoint(entry.file))
      impactDetails.push('entrypoint (discounted)');
    const impactSuffix =
      impactDetails.length > 0
        ? ` Architecture context: ${impactDetails.join(', ')}.`
        : '';

    const firstEffect = effects[0];
    if (!canAddFinding(findings)) break;
    findings.push({
      severity,
      category: 'import-side-effect-risk',
      file: entry.file,
      lineStart: firstEffect.lineStart,
      lineEnd: firstEffect.lineEnd,
      title: `Import-time side effect${effects.length > 1 ? `s (${effects.length})` : ''}: ${entry.file}`,
      reason: `Module executes work at import time: ${effectDetails}. Risk score: ${totalRisk} (ast=${astBase}, impact=+${impactBoost}, role=-${roleDiscount}). Confidence: ${confidence}.${impactSuffix}`,
      files: [entry.file],
      suggestedFix: {
        strategy:
          'Move import-time side effects behind explicit initialization or lazy loading.',
        steps: [
          'Wrap startup logic in an exported init() function instead of running at module scope.',
          'Replace synchronous I/O with async alternatives called at runtime.',
          'Guard side-effect imports with dynamic import() behind feature checks.',
          'If this is an intentional entrypoint, consider adding a suppression comment.',
        ],
      },
      impact: `Importing this module triggers ${effects.length} side effect(s). With fan-in=${fanIn}, unintended imports can degrade startup latency and cause surprising runtime behavior.`,
      tags: ['import-side-effect', 'startup', 'architecture', 'performance'],
      lspHints: [
        {
          tool: 'lspGetSemantics', semanticType: 'references',
          symbolName:
            entry.file
              .split('/')
              .pop()
              ?.replace(/\.[^.]+$/, '') || entry.file,
          lineHint: 1,
          file: entry.file,
          expectedResult: `find all modules that import this file and may trigger side effects`,
        },
      ],
    });
  }

  return findings;
}

export function detectNamespaceImport(
  dependencyState: DependencyState
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  for (const [
    file,
    imports,
  ] of dependencyState.importedSymbolsByFile.entries()) {
    if (isTestFile(file)) continue;

    for (const ref of imports) {
      if (ref.importedName !== '*') continue;
      if (ref.isTypeOnly) continue;
      if (ref.localName === 'require') continue;

      const isInternal = ref.resolvedModule != null;
      const fanIn = isInternal
        ? (dependencyState.incoming.get(ref.resolvedModule!) || new Set()).size
        : 0;

      findings.push({
        severity: isInternal && fanIn > 5 ? 'high' : 'medium',
        category: 'namespace-import',
        file,
        lineStart: ref.lineStart || 1,
        lineEnd: ref.lineEnd || ref.lineStart || 1,
        title: `Namespace import blocks tree-shaking: import * as ${ref.localName}`,
        reason: `\`import * as ${ref.localName} from '${ref.sourceModule}'\` forces bundlers to include the entire module. Named imports allow dead-code elimination of unused exports.${isInternal ? ` Target module has fan-in=${fanIn}.` : ''}`,
        files: [
          `${file}:${ref.lineStart || 1}-${ref.lineEnd || ref.lineStart || 1}`,
        ],
        suggestedFix: {
          strategy:
            'Replace namespace import with named imports for used symbols.',
          steps: [
            `Find which properties of \`${ref.localName}\` are actually accessed in this file.`,
            `Replace \`import * as ${ref.localName}\` with \`import { usedA, usedB } from '${ref.sourceModule}'\`.`,
            'If many properties are used, consider splitting the source module into smaller modules.',
          ],
        },
        impact:
          'Enables bundlers to tree-shake unused exports, reducing bundle size.',
        tags: ['tree-shaking', 'bundle-size', 'namespace-import'],
      });
    }
  }

  return findings;
}

export function detectCommonJsInEsm(
  dependencyState: DependencyState
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  for (const [
    file,
    imports,
  ] of dependencyState.importedSymbolsByFile.entries()) {
    if (isTestFile(file)) continue;

    const requireImports = imports.filter(
      r => r.localName === 'require' && !r.isTypeOnly
    );
    if (requireImports.length === 0) continue;

    const hasEsmImport = imports.some(r => r.localName !== 'require');
    const severity = hasEsmImport ? 'high' : 'medium';

    for (const ref of requireImports) {
      findings.push({
        severity,
        category: hasEsmImport ? 'mixed-module-format' : 'commonjs-in-esm',
        file,
        lineStart: ref.lineStart || 1,
        lineEnd: ref.lineEnd || ref.lineStart || 1,
        title: hasEsmImport
          ? `Mixed ESM/CJS: require('${ref.sourceModule}') in ESM file`
          : `CommonJS require blocks tree-shaking: require('${ref.sourceModule}')`,
        reason: hasEsmImport
          ? `File uses both ESM \`import\` and CJS \`require()\`. Mixed formats force bundlers to treat the module as CJS, disabling tree-shaking entirely. Found ${requireImports.length} require() call(s).`
          : `\`require('${ref.sourceModule}')\` is a CommonJS pattern that bundlers cannot statically analyze. ESM \`import\` enables tree-shaking.`,
        files: [
          `${file}:${ref.lineStart || 1}-${ref.lineEnd || ref.lineStart || 1}`,
        ],
        suggestedFix: {
          strategy: 'Convert require() to ESM import.',
          steps: [
            `Replace \`const mod = require('${ref.sourceModule}')\` with \`import mod from '${ref.sourceModule}'\` or named imports.`,
            'If the require is conditional, use dynamic `import()` instead.',
            'Ensure the target module supports ESM (check package.json "type" or "module" field).',
          ],
        },
        impact:
          'ESM imports enable tree-shaking; CJS requires pull the entire module.',
        tags: ['tree-shaking', 'bundle-size', 'commonjs', 'module-format'],
      });
    }
  }

  return findings;
}

export function detectExportStarLeak(
  dependencyState: DependencyState
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  for (const [file, reexports] of dependencyState.reExportsByFile.entries()) {
    if (isTestFile(file)) continue;

    const starReexports = reexports.filter(r => r.isStar && !r.isTypeOnly);
    if (starReexports.length === 0) continue;

    for (const ref of starReexports) {
      const targetExportCount = ref.resolvedModule
        ? (dependencyState.declaredExportsByFile.get(ref.resolvedModule) || [])
            .length
        : 0;

      const targetHasStars = ref.resolvedModule
        ? (dependencyState.reExportsByFile.get(ref.resolvedModule) || []).some(
            r => r.isStar
          )
        : false;

      const severity = targetHasStars
        ? 'high'
        : targetExportCount > 20
          ? 'high'
          : 'medium';

      findings.push({
        severity,
        category: 'export-star-leak',
        file,
        lineStart: ref.lineStart || 1,
        lineEnd: ref.lineEnd || ref.lineStart || 1,
        title: `export * leaks entire module surface: ${ref.sourceModule}`,
        reason: `\`export * from '${ref.sourceModule}'\` re-exports every symbol from the source, defeating granular tree-shaking.${targetExportCount > 0 ? ` Target exports ${targetExportCount} symbols.` : ''}${targetHasStars ? ' Target itself contains export-star chains, amplifying the leak.' : ''}`,
        files: [
          `${file}:${ref.lineStart || 1}-${ref.lineEnd || ref.lineStart || 1}`,
        ],
        suggestedFix: {
          strategy: 'Replace export * with explicit named re-exports.',
          steps: [
            `List the symbols actually consumed from \`${ref.sourceModule}\` by downstream modules.`,
            `Replace \`export * from '${ref.sourceModule}'\` with \`export { A, B, C } from '${ref.sourceModule}'\`.`,
            'This lets bundlers eliminate unused re-exports during tree-shaking.',
          ],
        },
        impact:
          'Explicit re-exports enable precise tree-shaking and make the public API surface visible.',
        tags: ['tree-shaking', 'bundle-size', 'export-star', 'api-surface'],
      });
    }
  }

  return findings;
}
