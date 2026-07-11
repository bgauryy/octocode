import { builtinModules } from 'node:module';

export const FALLBACK_SOURCE_EXTENSIONS = [
  '.ts',
  '.tsx',
  '.js',
  '.jsx',
  '.mjs',
  '.cjs',
  '.mts',
  '.cts',
] as const;

export const MANIFEST_NAME = 'package.json';

export const DEFAULT_EXCLUDE_DIRS = new Set([
  '.claude',
  '.context',
  '.cursor',
  '.git',
  'node_modules',
  'dist',
  'out',
  'coverage',
  'target',
  '.next',
  '.turbo',
  '.yarn',
]);

export const NODE_BUILTINS = new Set(
  builtinModules.flatMap(name => [name, `node:${name}`])
);

export type ResearchIntent =
  'general' | 'reachability' | 'dependencies' | 'symbols';

export type ResearchMode = 'plan' | 'analyze' | 'prove';

export type ResearchFlowStep = {
  readonly id: string;
  readonly purpose: string;
  readonly tools: readonly string[];
  readonly produces: readonly string[];
  readonly evidence: 'heuristic' | 'proof';
};

export type ResearchSymbolVerdict =
  | 'reachable'
  | 'candidate-unused-export'
  | 'transitive-dead'
  | 'unused-export'
  | 'unknown';

export type ResearchRetentionSource = 'ast' | 'ripgrep';

export type ResearchSymbolRow = {
  readonly symbol: string;
  readonly kind: string;
  readonly file: string;
  readonly line: number;
  readonly evidenceSource: 'ast' | 'regex';
  /** How cross-file retention was established. Prefer `ast`; `ripgrep` is lexical fallback only. */
  readonly retentionSource: ResearchRetentionSource;
  readonly directRefs: number;
  readonly externalRefs: number;
  readonly retainedBy: readonly string[];
  readonly verdict: ResearchSymbolVerdict;
};

export type ResearchGraphFactFile = {
  readonly file: string;
  readonly source: 'native-ast';
  readonly language: string;
  readonly declarations: readonly {
    readonly name: string;
    readonly kind: string;
    readonly line: number;
    readonly exported: boolean;
    readonly parent?: string;
  }[];
  readonly imports: readonly {
    readonly specifier: string;
    readonly line: number;
    readonly importKind: string;
    readonly localName?: string;
    readonly importedName?: string;
  }[];
  readonly exports: readonly {
    readonly name: string;
    readonly line: number;
    readonly exportKind: string;
    readonly localName?: string;
    readonly source?: string;
  }[];
  readonly calls: readonly {
    readonly caller: string;
    readonly callee: string;
    readonly line: number;
    readonly kind: string;
  }[];
  readonly edges: readonly {
    readonly from: string;
    readonly to: string;
    readonly relation: string;
    readonly source: string;
    readonly line: number;
  }[];
  readonly diagnostics: readonly string[];
};

export type ResearchFileIssue = {
  readonly kind: 'unusedFile';
  readonly file: string;
  readonly retainedBy: readonly string[];
  readonly verdict: 'unused-file';
};

export type ResearchDependencyIssue = {
  readonly kind:
    'unlistedDependency' | 'unusedDependency' | 'duplicateDependency';
  readonly packageName: string;
  readonly manifest: string;
  readonly usedBy: readonly string[];
  readonly declaredIn: readonly string[];
  readonly verdict: string;
};

export type ResearchManifestSummary = {
  readonly manifest: string;
  readonly name?: string;
  readonly entrypoints: readonly string[];
  readonly dependencyCount: number;
};

export type ResearchGraphCapabilitySummary = {
  readonly graphFactExtensions: readonly string[];
  readonly capabilityCount: number;
  readonly factFamilies: readonly string[];
  readonly sourceFilesByLanguage: Readonly<Record<string, number>>;
  readonly graphFilesByLanguage: Readonly<Record<string, number>>;
  readonly missingGraphFacts: readonly {
    readonly extension: string;
    readonly files: number;
    readonly reason: string;
  }[];
};

export type ResearchAnalysisResult = {
  readonly kind: 'researchFlow';
  readonly goal: string;
  readonly intent: ResearchIntent;
  readonly facets: readonly string[];
  readonly mode: ResearchMode;
  readonly root: string;
  readonly flow: readonly ResearchFlowStep[];
  readonly summary: {
    readonly manifests: number;
    readonly sourceFiles: number;
    readonly entrypoints: number;
    readonly reachableFiles: number;
    readonly unusedFiles: number;
    readonly unlistedDependencies: number;
    readonly unusedDependencies: number;
    readonly duplicateDependencies: number;
    readonly exportedSymbols: number;
    readonly candidateUnusedExports: number;
    readonly transitiveDeadExports: number;
    readonly nativeGraphFiles: number;
    readonly nativeGraphDeclarations: number;
    readonly nativeGraphCalls: number;
  };
  readonly manifests: readonly ResearchManifestSummary[];
  readonly files: readonly ResearchFileIssue[];
  readonly dependencies: readonly ResearchDependencyIssue[];
  readonly symbols: readonly ResearchSymbolRow[];
  readonly graphFacts: readonly ResearchGraphFactFile[];
  readonly graphCapabilities: ResearchGraphCapabilitySummary;
  readonly caveats: readonly string[];
};

export type AnalyzeResearchOptions = {
  readonly root: string;
  readonly goal?: string;
  readonly intent?: string;
  readonly facets?: readonly string[];
  readonly mode?: ResearchMode;
  readonly maxFiles?: number;
};

export type Manifest = {
  readonly path: string;
  readonly dir: string;
  readonly name?: string;
  readonly deps: ReadonlyMap<string, readonly string[]>;
  readonly entrypoints: readonly string[];
};

export type SourceFile = {
  readonly path: string;
  readonly rel: string;
  readonly extension: string;
  readonly language: string;
  readonly imports: readonly string[];
  readonly externalPackages: readonly string[];
  readonly graphFacts?: ResearchGraphFactFile;
};

export type ExportSymbol = {
  readonly symbol: string;
  readonly kind: string;
  readonly file: string;
  readonly line: number;
  readonly evidenceSource: 'ast' | 'regex';
};
