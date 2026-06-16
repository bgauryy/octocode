import type { GraphAnalyticsSummary } from '../analysis/graph-analytics.js';
import type {
  AnalysisLens,
  AnalysisSignal,
  BoundaryRoleHint,
  CfgFlags,
  EffectProfile,
  FileEntry,
  Finding,
  FlowTraceStep,
  HotFile,
  RecommendedValidation,
  SymbolUsageSummary,
} from '../types/index.js';

export interface ReportAnalysisSummary {
  graphSignals: AnalysisSignal[];
  astSignals: AnalysisSignal[];
  combinedSignals: AnalysisSignal[];
  strongestGraphSignal: AnalysisSignal | null;
  strongestAstSignal: AnalysisSignal | null;
  combinedInterpretation: AnalysisSignal | null;
  recommendedValidation: RecommendedValidation | null;
  investigationPrompts: string[];
}

function buildEffectProfile(entry: FileEntry): EffectProfile | undefined {
  const effects = entry.topLevelEffects || [];
  if (effects.length === 0) return undefined;
  const byKind: EffectProfile['byKind'] = {};
  let totalWeight = 0;
  let highestRisk: EffectProfile['highestRisk'] = null;
  let highestWeight = -1;
  for (const effect of effects) {
    byKind[effect.kind] = (byKind[effect.kind] || 0) + 1;
    totalWeight += effect.weight;
    if (effect.weight > highestWeight) {
      highestWeight = effect.weight;
      highestRisk = effect.kind;
    }
  }
  return {
    totalEffects: effects.length,
    totalWeight,
    byKind: byKind as EffectProfile['byKind'],
    highestRisk,
  };
}

function buildSymbolUsageSummary(entry: FileEntry): SymbolUsageSummary {
  const internalImports = entry.dependencyProfile.importedSymbols.filter(
    ref => !!ref.resolvedModule
  );
  const dominantInternalDependency =
    internalImports.length === 0
      ? null
      : [
          ...internalImports
            .reduce((acc, ref) => {
              const key = ref.resolvedModule || ref.sourceModule;
              acc.set(key, (acc.get(key) || 0) + 1);
              return acc;
            }, new Map<string, number>())
            .entries(),
        ].sort((a, b) => b[1] - a[1])[0]?.[0] || null;
  return {
    declaredExportCount: entry.dependencyProfile.declaredExports.length,
    importedSymbolCount: entry.dependencyProfile.importedSymbols.length,
    internalImportCount: internalImports.length,
    externalImportCount: entry.dependencyProfile.externalDependencies.length,
    reExportCount: entry.dependencyProfile.reExports.length,
    dominantInternalDependency,
  };
}

function buildBoundaryRoleHints(
  entry: FileEntry
): BoundaryRoleHint[] | undefined {
  const normalized = entry.file.replace(/\\/g, '/').toLowerCase();
  const hints: BoundaryRoleHint[] = [];
  const addHint = (
    role: string,
    confidence: BoundaryRoleHint['confidence'],
    reasons: string[]
  ): void => {
    hints.push({ role, confidence, reasons });
  };

  if (/(^|\/)(index|main|app|server|cli)\.[mc]?[jt]sx?$/.test(normalized)) {
    addHint('entrypoint', 'high', [
      'path matches a conventional entrypoint filename',
    ]);
  }
  if (/(^|\/)(components|ui|pages|screens)\//.test(normalized)) {
    addHint('ui', 'high', ['path indicates UI-facing code']);
  }
  if (/(^|\/)(routes|controllers|handlers|http|api)\//.test(normalized)) {
    addHint('transport', 'medium', [
      'path indicates controller, handler, or API transport code',
    ]);
  }
  if (/(^|\/)(service|services|use-cases|usecases)\//.test(normalized)) {
    addHint('service', 'medium', [
      'path indicates orchestration or service logic',
    ]);
  }
  if (
    /(^|\/)(repo|repos|repository|repositories|db|persistence)\//.test(
      normalized
    )
  ) {
    addHint('persistence', 'high', [
      'path indicates persistence or repository code',
    ]);
  }
  if ((entry.topLevelEffects?.length || 0) > 0) {
    addHint('runtime-bootstrap', 'medium', [
      'module has import-time side effects',
    ]);
  }
  if (
    entry.dependencyProfile.declaredExports.length >= 8 &&
    entry.dependencyProfile.importedSymbols.length <= 2
  ) {
    addHint('shared-utility', 'medium', [
      'exports many symbols and imports little internal behavior',
    ]);
  }

  if (hints.length === 0) return undefined;
  return hints.slice(0, 3);
}

function buildCfgFlags(entry: FileEntry): CfgFlags | undefined {
  const hasValidationChecks = (entry.inputSources || []).some(
    source => source.hasValidation
  );
  const hasCleanupHooks = !!entry.testProfile?.setupCalls.some(
    call => call.kind === 'afterAll' || call.kind === 'afterEach'
  );
  const exitPointCount = entry.functions.reduce(
    (sum, fn) => sum + fn.returns,
    0
  );
  const asyncBoundaryCount = entry.functions.reduce(
    (sum, fn) => sum + fn.awaits,
    0
  );
  const hasTopLevelEffects = (entry.topLevelEffects?.length || 0) > 0;
  return {
    hasValidationChecks,
    hasCleanupHooks,
    exitPointCount,
    asyncBoundaryCount,
    hasTopLevelEffects,
  };
}

export function enrichFileInventoryEntries(
  fileEntries: FileEntry[],
  options: { flowEnabled?: boolean } = {}
): FileEntry[] {
  const { flowEnabled = false } = options;
  return fileEntries.map(entry => ({
    ...entry,
    effectProfile: entry.effectProfile || buildEffectProfile(entry),
    symbolUsageSummary:
      entry.symbolUsageSummary || buildSymbolUsageSummary(entry),
    boundaryRoleHints:
      entry.boundaryRoleHints || buildBoundaryRoleHints(entry) || [],
    cfgFlags: flowEnabled ? entry.cfgFlags || buildCfgFlags(entry) : undefined,
  }));
}

function inferAnalysisLens(category: string): AnalysisLens {
  if (
    [
      'dependency-cycle',
      'dependency-critical-path',
      'architecture-sdp-violation',
      'high-coupling',
      'god-module-coupling',
      'orphan-module',
      'unreachable-module',
      'cycle-cluster',
      'broker-module',
      'bridge-module',
      'mega-folder',
    ].includes(category)
  )
    return 'graph';

  if (
    [
      'layer-violation',
      'low-cohesion',
      'feature-envy',
      'import-side-effect-risk',
      'package-boundary-chatter',
      'startup-risk-hub',
      'unvalidated-input-sink',
      'input-passthrough-risk',
      'missing-test-cleanup',
      'fake-timer-no-restore',
      'missing-mock-restoration',
    ].includes(category)
  )
    return 'hybrid';

  if (category.startsWith('dependency-')) return 'graph';
  if (category.startsWith('test-') || category === 'focused-test')
    return 'hybrid';
  if (
    [
      'hardcoded-secret',
      'eval-usage',
      'unsafe-html',
      'sql-injection-risk',
      'unsafe-regex',
      'prototype-pollution-risk',
      'path-traversal-risk',
      'command-injection-risk',
      'debug-log-leakage',
      'sensitive-data-logging',
    ].includes(category)
  ) {
    return 'hybrid';
  }

  if (
    [
      'over-abstraction',
      'concrete-dependency',
      'circular-type-dependency',
      'unused-parameter',
      'deep-override-chain',
      'interface-compliance',
      'unused-import',
      'orphan-implementation',
      'shotgun-surgery',
      'move-to-caller',
      'narrowable-type',
      'semantic-dead-export',
    ].includes(category)
  ) {
    return 'hybrid';
  }

  return 'ast';
}

function defaultConfidence(finding: Finding): Finding['confidence'] {
  if (finding.confidence) return finding.confidence;
  if (finding.severity === 'critical') return 'high';
  if (finding.analysisLens === 'graph') return 'medium';
  if (finding.analysisLens === 'hybrid') return 'medium';
  return 'low';
}

function parseFlowTraceSteps(
  finding: Finding,
  flowEnabled: boolean
): FlowTraceStep[] | undefined {
  if (!flowEnabled) return undefined;
  if (finding.flowTrace && finding.flowTrace.length > 0)
    return finding.flowTrace;
  const raw = finding.evidence?.propagationSteps;
  if (!Array.isArray(raw)) return undefined;
  const steps = raw
    .map(entry => {
      if (typeof entry !== 'string') return null;
      const match = entry.match(/^(.*?):(\d+)(?:-(\d+))?$/);
      if (!match) return null;
      return {
        file: match[1],
        lineStart: Number(match[2]),
        lineEnd: Number(match[3] || match[2]),
        label: 'propagation step',
      } satisfies FlowTraceStep;
    })
    .filter((step): step is FlowTraceStep => step !== null);
  return steps.length > 0 ? steps : undefined;
}

function buildRecommendedValidation(finding: Finding): RecommendedValidation {
  const primaryHint = finding.lspHints?.[0];
  if (primaryHint) {
    return {
      summary: primaryHint.expectedResult,
      tools: ['localSearchCode', primaryHint.tool],
    };
  }
  if (finding.analysisLens === 'graph') {
    return {
      summary:
        'Confirm the dependency edge or hub behavior with localSearchCode and an LSP navigation step.',
      tools: ['localSearchCode', 'lspGetSemantics(type=definition)'],
    };
  }
  if (finding.analysisLens === 'hybrid') {
    return {
      summary:
        'Validate both the structural location and the behavioral path before presenting the claim as fact.',
      tools: ['localSearchCode', 'lspGetSemantics(type=callers)'],
    };
  }
  return {
    summary:
      'Confirm the code location and inspect the matched structure before proposing a refactor.',
    tools: ['localSearchCode'],
  };
}

export function enrichFindings(
  findings: Finding[],
  fileEntries: FileEntry[],
  hotFiles: HotFile[],
  graphAnalytics: GraphAnalyticsSummary | null,
  options: { flowEnabled?: boolean } = {}
): Finding[] {
  const { flowEnabled = false } = options;
  const byFile = new Map(fileEntries.map(entry => [entry.file, entry]));
  const hotFileSet = new Set(hotFiles.map(entry => entry.file));
  const cycleFiles = new Set<string>();
  const criticalPathFiles = new Set<string>();
  if (graphAnalytics) {
    for (const cluster of graphAnalytics.sccClusters) {
      for (const file of cluster.files) cycleFiles.add(file);
    }
    for (const chokepoint of graphAnalytics.chokepoints) {
      if (chokepoint.onCriticalPath) criticalPathFiles.add(chokepoint.file);
    }
  }
  const relatedByFile = new Map<string, Set<string>>();
  for (const finding of findings) {
    if (!relatedByFile.has(finding.file))
      relatedByFile.set(finding.file, new Set());
    relatedByFile.get(finding.file)!.add(finding.category);
  }

  return findings.map(finding => {
    const analysisLens =
      finding.analysisLens || inferAnalysisLens(finding.category);
    const entry = byFile.get(finding.file);
    const correlatedSignals = new Set<string>(finding.correlatedSignals || []);
    if (hotFileSet.has(finding.file)) correlatedSignals.add('hot-file');
    if (cycleFiles.has(finding.file)) correlatedSignals.add('cycle-context');
    if (criticalPathFiles.has(finding.file))
      correlatedSignals.add('critical-path-context');
    if (entry?.effectProfile?.totalEffects)
      correlatedSignals.add('top-level-effects');
    for (const category of relatedByFile.get(finding.file) ||
      new Set<string>()) {
      if (category !== finding.category)
        correlatedSignals.add(`paired:${category}`);
    }

    const evidence = {
      category: finding.category,
      location: `${finding.file}:${finding.lineStart}-${finding.lineEnd}`,
      ...(finding.evidence || {}),
    };

    return {
      ...finding,
      ruleId: finding.ruleId || `${analysisLens}.${finding.category}`,
      analysisLens,
      confidence: defaultConfidence({ ...finding, analysisLens }),
      evidence,
      correlatedSignals: [...correlatedSignals].slice(0, 8),
      recommendedValidation:
        finding.recommendedValidation ||
        buildRecommendedValidation({ ...finding, analysisLens }),
      flowTrace: parseFlowTraceSteps({ ...finding, evidence }, flowEnabled),
    };
  });
}

function makeSignal(
  kind: string,
  lens: AnalysisLens,
  title: string,
  summary: string,
  confidence: AnalysisSignal['confidence'],
  score: number,
  files: string[],
  categories: string[],
  evidence: Record<string, unknown>
): AnalysisSignal {
  return {
    kind,
    lens,
    title,
    summary,
    confidence,
    score,
    files,
    categories,
    evidence,
  };
}

export function computeReportAnalysisSummary(
  findings: Finding[],
  fileEntries: FileEntry[],
  hotFiles: HotFile[],
  graphAnalytics: GraphAnalyticsSummary | null
): ReportAnalysisSummary {
  const categoriesByFile = new Map<string, Set<string>>();
  for (const finding of findings) {
    if (!categoriesByFile.has(finding.file))
      categoriesByFile.set(finding.file, new Set());
    categoriesByFile.get(finding.file)!.add(finding.category);
  }

  const graphSignals: AnalysisSignal[] = [];
  const astSignals: AnalysisSignal[] = [];

  if (graphAnalytics?.chokepoints.length) {
    const top = graphAnalytics.chokepoints[0];
    graphSignals.push(
      makeSignal(
        'structural-chokepoint',
        'graph',
        'Structural chokepoint',
        `${top.file} concentrates dependency pressure (${top.reasons.join(', ')}).`,
        top.articulation ? 'high' : 'medium',
        top.score,
        [top.file],
        ['broker-module', 'bridge-module'],
        { score: top.score, reasons: top.reasons }
      )
    );
  }

  if (graphAnalytics?.sccClusters.length) {
    const cluster = graphAnalytics.sccClusters[0];
    graphSignals.push(
      makeSignal(
        'cycle-cluster',
        'graph',
        'Cycle cluster',
        `${cluster.id} links ${cluster.nodeCount} files into a single strongly connected group.`,
        cluster.nodeCount >= 5 ? 'high' : 'medium',
        cluster.nodeCount * 6 + cluster.edgeCount,
        cluster.files,
        ['dependency-cycle', 'cycle-cluster'],
        {
          clusterId: cluster.id,
          nodeCount: cluster.nodeCount,
          hubFiles: cluster.hubFiles,
        }
      )
    );
  }

  if (graphAnalytics?.packageGraphSummary.hotspots.length) {
    const hotspot = graphAnalytics.packageGraphSummary.hotspots[0];
    graphSignals.push(
      makeSignal(
        'package-chatter',
        'graph',
        'Package boundary chatter',
        `${hotspot.from} and ${hotspot.to} exchange ${hotspot.edges} cross-package dependency edge(s).`,
        hotspot.edges >= 8 ? 'high' : 'medium',
        hotspot.edges * 4,
        [hotspot.from, hotspot.to],
        ['package-boundary-chatter'],
        hotspot
      )
    );
  }

  const megaFolderFindings = findings
    .filter(finding => finding.category === 'mega-folder')
    .sort((a, b) => {
      const aCount = Number(
        (a.evidence as Record<string, unknown> | undefined)?.fileCount || 0
      );
      const bCount = Number(
        (b.evidence as Record<string, unknown> | undefined)?.fileCount || 0
      );
      return bCount - aCount;
    });
  if (megaFolderFindings.length > 0) {
    const top = megaFolderFindings[0];
    const evidence =
      (top.evidence as Record<string, unknown> | undefined) || {};
    const folderPath =
      typeof evidence.folderPath === 'string'
        ? evidence.folderPath
        : folderOf(top.file);
    const fileCount = Number(evidence.fileCount || top.files.length || 0);
    const concentration = Number(evidence.concentration || 0);
    graphSignals.push(
      makeSignal(
        'mega-folder-cluster',
        'graph',
        'Mega folder concentration',
        `${folderPath} concentrates ${fileCount} files (${(concentration * 100).toFixed(1)}% of analyzed production files), which is a structural decomposition risk.`,
        concentration >= 0.5 || fileCount >= 50 ? 'high' : 'medium',
        Math.round(fileCount * 3 + concentration * 100),
        top.files.length > 0 ? top.files : [top.file],
        ['mega-folder'],
        {
          folderPath,
          fileCount,
          concentration,
        }
      )
    );
  }

  for (const entry of fileEntries) {
    const categories = categoriesByFile.get(entry.file) || new Set<string>();
    if (categories.has('low-cohesion') && categories.has('feature-envy')) {
      astSignals.push(
        makeSignal(
          'boundary-leak-shape',
          'ast',
          'Boundary leak shape',
          `${entry.file} shows both low cohesion and feature envy, suggesting the module boundary is doing multiple jobs.`,
          'high',
          90,
          [entry.file],
          ['low-cohesion', 'feature-envy'],
          { file: entry.file }
        )
      );
    }
    if (
      (entry.effectProfile?.totalEffects || 0) > 0 &&
      categories.has('import-side-effect-risk')
    ) {
      astSignals.push(
        makeSignal(
          'hidden-initialization',
          'ast',
          'Hidden initialization logic',
          `${entry.file} performs import-time work that matches the reported side-effect risk.`,
          'medium',
          75,
          [entry.file],
          ['import-side-effect-risk'],
          {
            totalEffects: entry.effectProfile?.totalEffects,
            highestRisk: entry.effectProfile?.highestRisk,
          }
        )
      );
    }
    if (
      categories.has('duplicate-flow-structure') &&
      categories.has('function-optimization')
    ) {
      astSignals.push(
        makeSignal(
          'orchestration-duplication',
          'ast',
          'Repeated orchestration shape',
          `${entry.file} combines repeated control-flow shape with complex functions, which usually means orchestration duplication.`,
          'medium',
          70,
          [entry.file],
          ['duplicate-flow-structure', 'function-optimization'],
          { file: entry.file }
        )
      );
    }
  }

  const strongestGraphSignal =
    graphSignals.sort((a, b) => b.score - a.score)[0] || null;
  const strongestAstSignal =
    astSignals.sort((a, b) => b.score - a.score)[0] || null;

  let combinedInterpretation: AnalysisSignal | null = null;
  if (strongestGraphSignal && strongestAstSignal) {
    const sharedFile = strongestGraphSignal.files.find(file =>
      strongestAstSignal.files.includes(file)
    );
    const confidence = sharedFile ? 'high' : 'medium';
    combinedInterpretation = makeSignal(
      'combined-interpretation',
      'hybrid',
      'Combined interpretation',
      sharedFile
        ? `${sharedFile} is both a structural hotspot and a suspicious code-shape hotspot, so it should be investigated first.`
        : `${strongestGraphSignal.title} and ${strongestAstSignal.title} both appear in this scan, so use a hybrid investigation instead of a single-lens conclusion.`,
      confidence,
      Math.round((strongestGraphSignal.score + strongestAstSignal.score) / 2),
      sharedFile
        ? [sharedFile]
        : [
            ...new Set([
              ...strongestGraphSignal.files,
              ...strongestAstSignal.files,
            ]),
          ].slice(0, 4),
      [
        ...new Set([
          ...strongestGraphSignal.categories,
          ...strongestAstSignal.categories,
        ]),
      ],
      {
        graphKind: strongestGraphSignal.kind,
        astKind: strongestAstSignal.kind,
        sharedFile: sharedFile || null,
      }
    );
  } else if (strongestGraphSignal || strongestAstSignal) {
    const signal = strongestGraphSignal || strongestAstSignal!;
    combinedInterpretation = makeSignal(
      'combined-interpretation',
      signal.lens,
      'Combined interpretation',
      signal.summary,
      signal.confidence,
      signal.score,
      signal.files,
      signal.categories,
      signal.evidence
    );
  }

  const relevantFiles = new Set(combinedInterpretation?.files || []);
  const relevantCategories = new Set(combinedInterpretation?.categories || []);
  const prioritizedFinding =
    findings.find(
      finding =>
        relevantFiles.has(finding.file) ||
        relevantCategories.has(finding.category)
    ) || findings[0];
  const recommendedValidation =
    prioritizedFinding?.recommendedValidation || null;

  const prompts = new Set<string>();
  if (strongestGraphSignal) {
    prompts.add(
      `Inspect ${strongestGraphSignal.files[0]} first and validate the graph claim with localSearchCode plus LSP navigation.`
    );
  }
  if (strongestAstSignal) {
    prompts.add(
      `Use file-inventory.json for ${strongestAstSignal.files[0]} to explain why the code shape matches the finding.`
    );
  }
  if (combinedInterpretation?.confidence === 'high') {
    prompts.add(
      'Treat the aligned graph and AST signal as an architecture priority, not just a local cleanup task.'
    );
  } else if (combinedInterpretation) {
    prompts.add(
      'Use a hybrid investigation before proposing a refactor because the signals do not fully align yet.'
    );
  }
  if (hotFiles.length > 0) {
    prompts.add(
      `Cross-check the top hotspot ${hotFiles[0].file} with the strongest architecture finding before editing code.`
    );
  }
  const megaFolderSignal = graphSignals.find(
    signal => signal.kind === 'mega-folder-cluster'
  );
  if (megaFolderSignal) {
    prompts.add(
      `Map the import graph of ${megaFolderSignal.evidence.folderPath}, identify domain clusters, and restructure with an automated migration script that moves files and rewrites import paths atomically. Validate with tsc + lint + tests after each phase.`
    );
  }

  return {
    graphSignals: graphSignals.sort((a, b) => b.score - a.score),
    astSignals: astSignals.sort((a, b) => b.score - a.score),
    combinedSignals: combinedInterpretation ? [combinedInterpretation] : [],
    strongestGraphSignal,
    strongestAstSignal,
    combinedInterpretation,
    recommendedValidation,
    investigationPrompts: [...prompts],
  };
}

function folderOf(filePath: string): string {
  const normalized = filePath.replace(/\\/g, '/');
  const idx = normalized.lastIndexOf('/');
  return idx === -1 ? '.' : normalized.slice(0, idx);
}
