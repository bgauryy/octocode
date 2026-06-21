import { builtinModules } from 'node:module';
import path from 'node:path';
import { readFile, readdir } from 'node:fs/promises';

const SOURCE_EXTENSIONS = new Set([
  '.ts',
  '.tsx',
  '.js',
  '.jsx',
  '.mjs',
  '.cjs',
  '.mts',
  '.cts',
]);

const MANIFEST_NAME = 'package.json';

const DEFAULT_EXCLUDE_DIRS = new Set([
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

const NODE_BUILTINS = new Set(
  builtinModules.flatMap(name => [name, `node:${name}`])
);

export type ResearchIntent =
  | 'general'
  | 'reachability'
  | 'dependencies'
  | 'symbols';

export type ResearchMode = 'plan' | 'analyze';

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

export type ResearchSymbolRow = {
  readonly symbol: string;
  readonly kind: string;
  readonly file: string;
  readonly line: number;
  readonly directRefs: number;
  readonly externalRefs: number;
  readonly retainedBy: readonly string[];
  readonly verdict: ResearchSymbolVerdict;
};

export type ResearchFileIssue = {
  readonly kind: 'unusedFile';
  readonly file: string;
  readonly retainedBy: readonly string[];
  readonly verdict: 'unused-file';
};

export type ResearchDependencyIssue = {
  readonly kind:
    | 'unlistedDependency'
    | 'unusedDependency'
    | 'duplicateDependency';
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
  };
  readonly manifests: readonly ResearchManifestSummary[];
  readonly files: readonly ResearchFileIssue[];
  readonly dependencies: readonly ResearchDependencyIssue[];
  readonly symbols: readonly ResearchSymbolRow[];
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

type Manifest = {
  readonly path: string;
  readonly dir: string;
  readonly name?: string;
  readonly deps: ReadonlyMap<string, readonly string[]>;
  readonly entrypoints: readonly string[];
};

type SourceFile = {
  readonly path: string;
  readonly rel: string;
  readonly imports: readonly string[];
  readonly externalPackages: readonly string[];
};

type ExportSymbol = {
  readonly symbol: string;
  readonly kind: string;
  readonly file: string;
  readonly line: number;
};

export async function analyzeResearchFlow(
  options: AnalyzeResearchOptions
): Promise<ResearchAnalysisResult> {
  const root = path.resolve(options.root);
  const mode = options.mode ?? 'analyze';
  const goal = options.goal?.trim() || 'Analyze this repository.';
  const intent = inferIntent(goal, options.intent, options.facets);
  const facets = normalizeFacets(intent, options.facets);
  const flow = buildResearchFlow(intent, facets);

  if (mode === 'plan') {
    return emptyResult({ root, goal, intent, facets, mode, flow });
  }

  const files = await walkFiles(root, options.maxFiles ?? 5000);
  const manifests = await readManifests(
    files.filter(file => file.endsWith(MANIFEST_NAME))
  );
  const sourceFiles = await readSourceFiles(root, files);
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
    caveats: [
      'This is a smart research flow with heuristic graph evidence. LSP references and structural AST refinement should be used before destructive cleanup.',
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
        tools: [
          'localSearchCode structural',
          'lspGetSemantics documentSymbols',
        ],
        produces: ['symbols', 'lineHints'],
        evidence: 'proof',
      },
      {
        id: 'reference-proof',
        purpose:
          'Use references grouped by file to separate direct, external, and transitive retention.',
        tools: ['lspGetSemantics references', 'localSearchCode'],
        produces: [
          'directRefs',
          'externalRefs',
          'retainedBy',
          'transitiveDead',
        ],
        evidence: 'proof',
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
    },
    manifests: [],
    files: [],
    dependencies: [],
    symbols: [],
    caveats: [
      'Planning mode returned the research flow without scanning files.',
    ],
  };
}

async function walkFiles(root: string, maxFiles: number): Promise<string[]> {
  const out: string[] = [];
  async function visit(dir: string): Promise<void> {
    if (out.length >= maxFiles) return;
    const entries = await readdir(dir, { withFileTypes: true });
    for (const entry of entries) {
      if (out.length >= maxFiles) return;
      if (entry.isDirectory()) {
        if (DEFAULT_EXCLUDE_DIRS.has(entry.name)) continue;
        await visit(path.join(dir, entry.name));
        continue;
      }
      if (entry.isFile()) out.push(path.join(dir, entry.name));
    }
  }
  await visit(root);
  return out;
}

async function readManifests(paths: readonly string[]): Promise<Manifest[]> {
  const manifests = await Promise.all(
    paths.map(async manifestPath => parseManifest(manifestPath))
  );
  return manifests.filter(
    (manifest): manifest is Manifest => manifest !== null
  );
}

async function parseManifest(manifestPath: string): Promise<Manifest | null> {
  const raw = await readJsonObject(manifestPath);
  if (!raw) return null;
  const dir = path.dirname(manifestPath);
  const deps = new Map<string, readonly string[]>();
  for (const field of [
    'dependencies',
    'devDependencies',
    'peerDependencies',
    'optionalDependencies',
  ]) {
    const values = recordValue(raw[field]);
    if (!values) continue;
    for (const name of Object.keys(values)) {
      deps.set(name, [...(deps.get(name) ?? []), field]);
    }
  }
  return {
    path: manifestPath,
    dir,
    name: stringValue(raw.name),
    deps,
    entrypoints: manifestEntrypoints(dir, raw),
  };
}

async function readJsonObject(
  file: string
): Promise<Record<string, unknown> | null> {
  try {
    const parsed = JSON.parse(await readFile(file, 'utf8')) as unknown;
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed)
      ? (parsed as Record<string, unknown>)
      : null;
  } catch {
    return null;
  }
}

function manifestEntrypoints(
  dir: string,
  raw: Record<string, unknown>
): readonly string[] {
  const candidates = new Set<string>();
  for (const field of ['main', 'module', 'types', 'typings']) {
    const value = stringValue(raw[field]);
    if (value) candidates.add(path.resolve(dir, value));
  }
  const bin = raw.bin;
  if (typeof bin === 'string') candidates.add(path.resolve(dir, bin));
  if (bin && typeof bin === 'object' && !Array.isArray(bin)) {
    for (const value of Object.values(bin)) {
      if (typeof value === 'string') candidates.add(path.resolve(dir, value));
    }
  }
  collectExportsEntrypoints(dir, raw.exports, candidates);
  for (const fallback of [
    'src/index.ts',
    'src/index.tsx',
    'src/index.js',
    'index.ts',
    'index.js',
  ]) {
    candidates.add(path.resolve(dir, fallback));
  }
  return [...candidates];
}

function collectExportsEntrypoints(
  dir: string,
  value: unknown,
  out: Set<string>
): void {
  if (typeof value === 'string') {
    out.add(path.resolve(dir, value));
    return;
  }
  if (!value || typeof value !== 'object' || Array.isArray(value)) return;
  for (const child of Object.values(value as Record<string, unknown>)) {
    collectExportsEntrypoints(dir, child, out);
  }
}

async function readSourceFiles(
  root: string,
  files: readonly string[]
): Promise<SourceFile[]> {
  const sourcePaths = files.filter(file =>
    SOURCE_EXTENSIONS.has(path.extname(file))
  );
  const records = await Promise.all(
    sourcePaths.map(async file => {
      const text = await readFile(file, 'utf8').catch(() => '');
      const imports = importSpecifiers(text);
      return {
        path: file,
        rel: relative(root, file),
        imports,
        externalPackages: imports
          .filter(specifier => !isRelativeSpecifier(specifier))
          .map(packageNameFromSpecifier)
          .filter(isNonEmptyString)
          .filter(name => !NODE_BUILTINS.has(name)),
      };
    })
  );
  return records;
}

function importSpecifiers(text: string): readonly string[] {
  const specs: string[] = [];
  const patterns = [
    /\bimport\s+(?:type\s+)?(?:[^'"]*?\s+from\s+)?['"]([^'"]+)['"]/g,
    /\bexport\s+(?:type\s+)?[^'"]*?\s+from\s+['"]([^'"]+)['"]/g,
    /\bimport\s*\(\s*['"]([^'"]+)['"]\s*\)/g,
    /\brequire\s*\(\s*['"]([^'"]+)['"]\s*\)/g,
  ];
  for (const pattern of patterns) {
    for (const match of text.matchAll(pattern)) {
      if (match[1]) specs.push(match[1]);
    }
  }
  return specs;
}

function buildFileGraph(
  sourceFiles: readonly SourceFile[]
): ReadonlyMap<string, readonly string[]> {
  const known = new Set(sourceFiles.map(file => file.path));
  const byDir = new Map(
    sourceFiles.map(file => [file.path, path.dirname(file.path)])
  );
  const graph = new Map<string, string[]>();
  for (const file of sourceFiles) {
    const deps: string[] = [];
    const dir = byDir.get(file.path) ?? path.dirname(file.path);
    for (const specifier of file.imports) {
      if (!isRelativeSpecifier(specifier)) continue;
      const resolved = resolveImport(dir, specifier, known);
      if (resolved) deps.push(resolved);
    }
    graph.set(file.path, deps);
  }
  return graph;
}

function collectEntrypoints(
  root: string,
  manifests: readonly Manifest[],
  sourceFiles: readonly SourceFile[]
): readonly string[] {
  const known = new Set(sourceFiles.map(file => file.path));
  const out = new Set<string>();
  for (const manifest of manifests) {
    for (const entry of manifest.entrypoints) {
      const resolved = resolveExistingPath(entry, known);
      if (resolved) out.add(resolved);
    }
  }
  for (const fallback of [
    'src/index.ts',
    'src/index.tsx',
    'src/index.js',
    'index.ts',
    'index.js',
  ]) {
    const resolved = resolveExistingPath(path.resolve(root, fallback), known);
    if (resolved) out.add(resolved);
  }
  return [...out];
}

function reachableFiles(
  entrypoints: readonly string[],
  graph: ReadonlyMap<string, readonly string[]>
): ReadonlySet<string> {
  const seen = new Set<string>();
  const stack = [...entrypoints];
  while (stack.length > 0) {
    const file = stack.pop();
    if (!file || seen.has(file)) continue;
    seen.add(file);
    for (const dep of graph.get(file) ?? []) stack.push(dep);
  }
  return seen;
}

function collectDependencyIssues(
  root: string,
  manifests: readonly Manifest[],
  sourceFiles: readonly SourceFile[],
  workspacePackages: ReadonlySet<string>
): readonly ResearchDependencyIssue[] {
  const byManifest = new Map<string, Set<string>>();
  const usedByPackage = new Map<string, Set<string>>();
  for (const file of sourceFiles) {
    const manifest = nearestManifest(file.path, manifests);
    if (!manifest) continue;
    const used = byManifest.get(manifest.path) ?? new Set<string>();
    byManifest.set(manifest.path, used);
    for (const packageName of file.externalPackages) {
      used.add(packageName);
      const files =
        usedByPackage.get(`${manifest.path}\0${packageName}`) ??
        new Set<string>();
      files.add(file.rel);
      usedByPackage.set(`${manifest.path}\0${packageName}`, files);
    }
  }

  const issues: ResearchDependencyIssue[] = [];
  for (const manifest of manifests) {
    const used = byManifest.get(manifest.path) ?? new Set<string>();
    for (const packageName of used) {
      if (
        workspacePackages.has(packageName) ||
        manifest.deps.has(packageName)
      ) {
        continue;
      }
      issues.push({
        kind: 'unlistedDependency',
        packageName,
        manifest: relative(root, manifest.path),
        usedBy: [
          ...(usedByPackage.get(`${manifest.path}\0${packageName}`) ?? []),
        ],
        declaredIn: [],
        verdict: 'unlisted-dependency',
      });
    }
    for (const [packageName, fields] of manifest.deps) {
      if (fields.length > 1) {
        issues.push({
          kind: 'duplicateDependency',
          packageName,
          manifest: relative(root, manifest.path),
          usedBy: [
            ...(usedByPackage.get(`${manifest.path}\0${packageName}`) ?? []),
          ],
          declaredIn: fields,
          verdict: 'duplicate-dependency',
        });
      }
      if (!used.has(packageName) && !workspacePackages.has(packageName)) {
        issues.push({
          kind: 'unusedDependency',
          packageName,
          manifest: relative(root, manifest.path),
          usedBy: [],
          declaredIn: fields,
          verdict: 'candidate-unused-dependency',
        });
      }
    }
  }
  return issues;
}

function nearestManifest(
  file: string,
  manifests: readonly Manifest[]
): Manifest | undefined {
  const candidates = manifests
    .filter(manifest => file.startsWith(`${manifest.dir}${path.sep}`))
    .sort((a, b) => b.dir.length - a.dir.length);
  return candidates[0];
}

async function collectExportSymbols(
  sourceFiles: readonly SourceFile[]
): Promise<ExportSymbol[]> {
  const rows = await Promise.all(
    sourceFiles.map(async file => {
      const text = await readFile(file.path, 'utf8').catch(() => '');
      return exportSymbols(file.rel, text);
    })
  );
  return rows.flat();
}

function exportSymbols(file: string, text: string): readonly ExportSymbol[] {
  const symbols: ExportSymbol[] = [];
  const lines = text.split(/\r?\n/);
  for (let index = 0; index < lines.length; index++) {
    const line = lines[index] ?? '';
    const named =
      /\bexport\s+(?:async\s+)?(?:function|class|const|let|var|type|interface|enum)\s+([A-Za-z_$][\w$]*)/.exec(
        line
      );
    if (named) {
      symbols.push({
        symbol: named[1]!,
        kind: exportKind(line),
        file,
        line: index + 1,
      });
    }
    const list = /\bexport\s*\{([^}]+)\}/.exec(line);
    if (list) {
      for (const part of list[1]!.split(',')) {
        const symbol = part
          .trim()
          .split(/\s+as\s+/)[0]
          ?.trim();
        if (symbol && /^[A-Za-z_$][\w$]*$/.test(symbol)) {
          symbols.push({ symbol, kind: 'export', file, line: index + 1 });
        }
      }
    }
  }
  return symbols;
}

function scoreSymbols(
  symbols: readonly ExportSymbol[],
  sourceFiles: readonly SourceFile[],
  reachable: ReadonlySet<string>
): readonly ResearchSymbolRow[] {
  const sourceByRel = new Map(sourceFiles.map(file => [file.rel, file]));
  return symbols.map(symbol => {
    const refs = sourceFiles.filter(
      file => file.rel !== symbol.file && tokenAppears(file.path, symbol.symbol)
    );
    const reachableRefs = refs.filter(file => reachable.has(file.path));
    const declaringFile = sourceByRel.get(symbol.file);
    const declaringReachable = declaringFile
      ? reachable.has(declaringFile.path)
      : false;
    const verdict: ResearchSymbolVerdict =
      reachableRefs.length > 0
        ? 'reachable'
        : refs.length > 0
          ? 'transitive-dead'
          : declaringReachable
            ? 'candidate-unused-export'
            : 'unused-export';
    return {
      symbol: symbol.symbol,
      kind: symbol.kind,
      file: symbol.file,
      line: symbol.line,
      directRefs: refs.length,
      externalRefs: reachableRefs.length,
      retainedBy: refs.map(file => file.rel),
      verdict,
    };
  });
}

function tokenAppears(file: string, token: string): boolean {
  return fileContentCache.get(file)?.has(token) ?? false;
}

const fileContentCache = new Map<string, Set<string>>();

async function cacheTokens(sourceFiles: readonly SourceFile[]): Promise<void> {
  fileContentCache.clear();
  await Promise.all(
    sourceFiles.map(async file => {
      const text = await readFile(file.path, 'utf8').catch(() => '');
      fileContentCache.set(
        file.path,
        new Set(text.match(/[A-Za-z_$][\w$]*/g) ?? [])
      );
    })
  );
}

function exportKind(line: string): string {
  const match = /\b(function|class|const|let|var|type|interface|enum)\b/.exec(
    line
  );
  return match?.[1] ?? 'export';
}

function resolveImport(
  dir: string,
  specifier: string,
  known: ReadonlySet<string>
): string | undefined {
  return resolveExistingPath(path.resolve(dir, specifier), known);
}

function resolveExistingPath(
  base: string,
  known: ReadonlySet<string>
): string | undefined {
  const candidates = [base];
  for (const ext of SOURCE_EXTENSIONS) candidates.push(`${base}${ext}`);
  for (const ext of SOURCE_EXTENSIONS)
    candidates.push(path.join(base, `index${ext}`));
  return candidates.find(candidate => known.has(candidate));
}

function isRelativeSpecifier(specifier: string): boolean {
  return specifier.startsWith('./') || specifier.startsWith('../');
}

function packageNameFromSpecifier(specifier: string): string | undefined {
  if (specifier.startsWith('@')) {
    const [scope, name] = specifier.split('/');
    return scope && name ? `${scope}/${name}` : undefined;
  }
  return specifier.split('/')[0];
}

function recordValue(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined;
}

function stringValue(value: unknown): string | undefined {
  return typeof value === 'string' && value.trim() ? value : undefined;
}

function isNonEmptyString(value: string | undefined): value is string {
  return typeof value === 'string' && value.length > 0;
}

function relative(root: string, file: string): string {
  const rel = path.relative(root, file);
  return rel || path.basename(file);
}
