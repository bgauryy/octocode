import path from 'node:path';
import {
  collectDependencyIssues,
  readManifests,
  walkFiles,
} from './analyze/manifest-scan.js';
import {
  buildFileGraph,
  collectEntrypoints,
  reachableFiles,
  readSourceFiles,
  summarizeGraphCapabilities,
} from './analyze/source-graph.js';
import {
  cacheTokens,
  collectExportSymbols,
  scoreSymbols,
} from './analyze/symbol-scoring.js';
import { MANIFEST_NAME } from './analyze/types.js';
import {
  graphFactCapabilities,
  isNonEmptyString,
  relative,
} from './analyze/utils.js';
import type {
  AnalyzeResearchOptions,
  ResearchAnalysisResult,
  ResearchFlowStep,
  ResearchGraphCapabilitySummary,
  ResearchGraphFactFile,
  ResearchIntent,
  ResearchMode,
} from './analyze/types.js';

export type {
  AnalyzeResearchOptions,
  ResearchAnalysisResult,
  ResearchDependencyIssue,
  ResearchFileIssue,
  ResearchFlowStep,
  ResearchGraphCapabilitySummary,
  ResearchGraphFactFile,
  ResearchIntent,
  ResearchManifestSummary,
  ResearchMode,
  ResearchRetentionSource,
  ResearchSymbolRow,
  ResearchSymbolVerdict,
} from './analyze/types.js';

export async function analyzeResearchFlow(
  options: AnalyzeResearchOptions
): Promise<ResearchAnalysisResult> {
  const root = path.resolve(options.root);
  const mode = options.mode ?? 'analyze';
  const goal = options.goal?.trim() || 'Analyze this repository.';
  const intent = inferIntent(goal, options.intent, options.facets);
  const facets = normalizeFacets(intent, options.facets);
  const flow = buildResearchFlow(intent, facets);
  const capabilityMatrix = graphFactCapabilities();

  if (mode === 'plan') {
    return emptyResult({
      root,
      goal,
      intent,
      facets,
      mode,
      flow,
      graphCapabilities: summarizeGraphCapabilities([], [], capabilityMatrix),
    });
  }

  const files = await walkFiles(root, options.maxFiles ?? 5000);
  const manifests = await readManifests(
    files.filter(file => file.endsWith(MANIFEST_NAME))
  );
  const sourceFiles = await readSourceFiles(root, files);
  const graphFacts = sourceFiles
    .map(file => file.graphFacts)
    .filter((facts): facts is ResearchGraphFactFile => facts !== undefined);
  const graphCapabilities = summarizeGraphCapabilities(
    sourceFiles,
    graphFacts,
    capabilityMatrix
  );
  await cacheTokens(sourceFiles);
  const workspacePackages = new Set(
    manifests.map(manifest => manifest.name).filter(isNonEmptyString)
  );
  const graph = buildFileGraph(sourceFiles);
  const entrypoints = collectEntrypoints(root, manifests, sourceFiles);
  const reachable = reachableFiles(entrypoints, graph);
  const fileIssues = sourceFiles
    .filter(file => !reachable.has(file.path))
    .map(file => ({
      kind: 'unusedFile' as const,
      file: file.rel,
      retainedBy: [] as readonly string[],
      verdict: 'unused-file' as const,
    }));
  const dependencyIssues = collectDependencyIssues(
    root,
    manifests,
    sourceFiles,
    workspacePackages
  );
  const symbols = await collectExportSymbols(sourceFiles);
  const symbolRows = scoreSymbols(symbols, sourceFiles, reachable);
  const candidateUnusedExports = symbolRows.filter(
    row =>
      row.verdict === 'candidate-unused-export' ||
      row.verdict === 'unused-export'
  ).length;
  const transitiveDeadExports = symbolRows.filter(
    row => row.verdict === 'transitive-dead'
  ).length;

  return {
    kind: 'researchFlow',
    goal,
    intent,
    facets,
    mode,
    root,
    flow,
    summary: {
      manifests: manifests.length,
      sourceFiles: sourceFiles.length,
      entrypoints: entrypoints.length,
      reachableFiles: reachable.size,
      unusedFiles: fileIssues.length,
      unlistedDependencies: dependencyIssues.filter(
        issue => issue.kind === 'unlistedDependency'
      ).length,
      unusedDependencies: dependencyIssues.filter(
        issue => issue.kind === 'unusedDependency'
      ).length,
      duplicateDependencies: dependencyIssues.filter(
        issue => issue.kind === 'duplicateDependency'
      ).length,
      exportedSymbols: symbolRows.length,
      candidateUnusedExports,
      transitiveDeadExports,
      nativeGraphFiles: graphFacts.length,
      nativeGraphDeclarations: graphFacts.reduce(
        (total, facts) => total + facts.declarations.length,
        0
      ),
      nativeGraphCalls: graphFacts.reduce(
        (total, facts) => total + facts.calls.length,
        0
      ),
    },
    manifests: manifests.map(manifest => ({
      manifest: relative(root, manifest.path),
      ...(manifest.name ? { name: manifest.name } : {}),
      entrypoints: manifest.entrypoints.map(entry => relative(root, entry)),
      dependencyCount: [...manifest.deps.keys()].length,
    })),
    files: fileIssues,
    dependencies: dependencyIssues,
    symbols: symbolRows,
    graphFacts,
    graphCapabilities,
    caveats: [
      'Analyze uses native AST graph facts for inventory and preferred cross-file retention (named imports / call callees). Lexical token scan is fallback only when AST finds no retainers.',
      'Graph facts are syntax inventory, not semantic identity. Upgrade candidates with target:graph proof:"lsp" (or lspGetSemantics references/callers) before destructive cleanup.',
      'Dynamic imports, framework entrypoints, generated files, test-only retention, and package-manager-specific workspace rules may require project-specific refinement.',
    ],
  };
}

function inferIntent(
  goal: string,
  explicit: string | undefined,
  facets: readonly string[] | undefined
): ResearchIntent {
  const text =
    `${explicit ?? ''} ${facets?.join(' ') ?? ''} ${goal}`.toLowerCase();
  if (/dead|unused|reachab|entry\s*point|transitive/.test(text)) {
    return 'reachability';
  }
  if (/dep|package|manifest|unlisted|duplicate/.test(text)) {
    return 'dependencies';
  }
  if (/symbol|export|reference|lsp|ast/.test(text)) {
    return 'symbols';
  }
  return 'general';
}

function normalizeFacets(
  intent: ResearchIntent,
  facets: readonly string[] | undefined
): readonly string[] {
  if (facets && facets.length > 0) return [...new Set(facets)];
  switch (intent) {
    case 'reachability':
      return ['entrypoints', 'files', 'symbols', 'dependencies'];
    case 'dependencies':
      return ['manifests', 'dependencies', 'imports'];
    case 'symbols':
      return ['symbols', 'references', 'ast', 'lsp'];
    case 'general':
      return ['structure', 'symbols', 'dependencies'];
  }
}

function buildResearchFlow(
  intent: ResearchIntent,
  facets: readonly string[]
): readonly ResearchFlowStep[] {
  const base: ResearchFlowStep[] = [
    {
      id: 'orient',
      purpose: 'Discover manifests, source files, and analysis boundaries.',
      tools: ['localFindFiles', 'localViewStructure'],
      produces: ['fileUniverse', 'manifestSet'],
      evidence: 'proof',
    },
    {
      id: 'manifest-graph',
      purpose:
        'Extract package entrypoints, declared dependencies, workspace package names, and script hints.',
      tools: ['package.json parser'],
      produces: ['entrypoints', 'declaredDependencies', 'workspacePackages'],
      evidence: 'heuristic',
    },
  ];
  if (intent === 'reachability' || facets.includes('symbols')) {
    base.push(
      {
        id: 'symbol-inventory',
        purpose:
          'Enumerate exports and declaration anchors for symbol-level questions.',
        tools: ['extractGraphFacts', 'export regex fallback'],
        produces: ['symbols', 'lineHints'],
        evidence: 'heuristic',
      },
      {
        id: 'reference-scan',
        purpose:
          'Candidate cross-file retention from native AST imports/calls (preferred) with lexical token fallback. Upgrade via target:graph proof:lsp — analyze does not run LSP.',
        tools: ['extractGraphFacts', 'token scan fallback'],
        produces: [
          'directRefs',
          'externalRefs',
          'retainedBy',
          'transitiveDead',
        ],
        evidence: 'heuristic',
      }
    );
  }
  if (intent === 'dependencies' || facets.includes('dependencies')) {
    base.push({
      id: 'dependency-audit',
      purpose:
        'Compare import specifiers with package manifests to find unlisted, unused, and duplicate dependencies.',
      tools: ['package.json parser', 'import graph'],
      produces: [
        'unlistedDependencies',
        'unusedDependencies',
        'duplicateDependencies',
      ],
      evidence: 'heuristic',
    });
  }
  return base;
}

function emptyResult(input: {
  readonly root: string;
  readonly goal: string;
  readonly intent: ResearchIntent;
  readonly facets: readonly string[];
  readonly mode: ResearchMode;
  readonly flow: readonly ResearchFlowStep[];
  readonly graphCapabilities: ResearchGraphCapabilitySummary;
}): ResearchAnalysisResult {
  return {
    kind: 'researchFlow',
    goal: input.goal,
    intent: input.intent,
    facets: input.facets,
    mode: input.mode,
    root: input.root,
    flow: input.flow,
    summary: {
      manifests: 0,
      sourceFiles: 0,
      entrypoints: 0,
      reachableFiles: 0,
      unusedFiles: 0,
      unlistedDependencies: 0,
      unusedDependencies: 0,
      duplicateDependencies: 0,
      exportedSymbols: 0,
      candidateUnusedExports: 0,
      transitiveDeadExports: 0,
      nativeGraphFiles: 0,
      nativeGraphDeclarations: 0,
      nativeGraphCalls: 0,
    },
    manifests: [],
    files: [],
    dependencies: [],
    symbols: [],
    graphFacts: [],
    graphCapabilities: input.graphCapabilities,
    caveats: [
      'Planning mode returned the research flow without scanning files.',
    ],
  };
}
