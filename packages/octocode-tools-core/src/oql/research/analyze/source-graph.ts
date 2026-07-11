import path from 'node:path';
import { readFile } from 'node:fs/promises';
import {
  contextUtils,
  type GraphFactCapability,
  type GraphFacts,
} from '../../../utils/contextUtils.js';
import {
  NODE_BUILTINS,
  type Manifest,
  type ResearchGraphCapabilitySummary,
  type ResearchGraphFactFile,
  type SourceFile,
} from './types.js';
import {
  countBy,
  isNonEmptyString,
  isRelativeSpecifier,
  isString,
  packageNameFromSpecifier,
  relative,
  resolveExistingPath,
  resolveImport,
  sourceExtensions,
  uniqueStrings,
} from './utils.js';

export async function readSourceFiles(
  root: string,
  files: readonly string[]
): Promise<SourceFile[]> {
  const supportedExtensions = sourceExtensions();
  const sourcePaths = files.filter(file =>
    supportedExtensions.has(path.extname(file).toLowerCase())
  );
  const records = await Promise.all(
    sourcePaths.map(async file => {
      const text = await readFile(file, 'utf8').catch(() => '');
      const extension = path.extname(file).toLowerCase();
      const graphFacts = extractNativeGraphFacts(
        text,
        file,
        relative(root, file)
      );
      const imports = uniqueStrings(
        graphFacts
          ? graphFacts.imports.map(item => item.specifier)
          : importSpecifiers(text)
      );
      return {
        path: file,
        rel: relative(root, file),
        extension,
        language: graphFacts?.language ?? extension.slice(1),
        imports,
        externalPackages: imports
          .filter(specifier => !isRelativeSpecifier(specifier))
          .map(packageNameFromSpecifier)
          .filter(isNonEmptyString)
          .filter(name => !NODE_BUILTINS.has(name)),
        ...(graphFacts ? { graphFacts } : {}),
      };
    })
  );
  return records;
}

function extractNativeGraphFacts(
  text: string,
  file: string,
  rel: string
): ResearchGraphFactFile | undefined {
  try {
    const raw = contextUtils.extractGraphFacts(text, file);
    if (!raw) return undefined;
    const parsed = JSON.parse(raw) as Partial<GraphFacts>;
    if (
      parsed.kind !== 'graphFacts' ||
      parsed.source !== 'native-ast' ||
      !Array.isArray(parsed.declarations) ||
      !Array.isArray(parsed.imports) ||
      !Array.isArray(parsed.exports) ||
      !Array.isArray(parsed.calls) ||
      !Array.isArray(parsed.edges)
    ) {
      return undefined;
    }
    return {
      file: rel,
      source: parsed.source,
      language: parsed.language ?? path.extname(file).slice(1),
      declarations: parsed.declarations.map(decl => ({
        name: decl.name,
        kind: decl.kind,
        line: decl.line,
        exported: decl.exported,
        ...(decl.parent ? { parent: decl.parent } : {}),
      })),
      imports: parsed.imports.map(item => ({
        specifier: item.specifier,
        line: item.line,
        importKind: item.importKind,
        ...(item.localName ? { localName: item.localName } : {}),
        ...(item.importedName ? { importedName: item.importedName } : {}),
      })),
      exports: parsed.exports.map(item => ({
        name: item.name,
        line: item.line,
        exportKind: item.exportKind,
        ...(item.localName ? { localName: item.localName } : {}),
        ...(item.source ? { source: item.source } : {}),
      })),
      calls: parsed.calls.map(call => ({
        caller: call.caller,
        callee: call.callee,
        line: call.line,
        kind: call.kind,
      })),
      edges: parsed.edges.map(edge => ({
        from: edge.from,
        to: edge.to,
        relation: edge.relation,
        source: edge.source,
        line: edge.line,
      })),
      diagnostics: Array.isArray(parsed.diagnostics)
        ? parsed.diagnostics.filter(isString)
        : [],
    };
  } catch {
    return undefined;
  }
}

export function summarizeGraphCapabilities(
  sourceFiles: readonly SourceFile[],
  graphFacts: readonly ResearchGraphFactFile[],
  capabilities: readonly GraphFactCapability[]
): ResearchGraphCapabilitySummary {
  const graphFactExtensions =
    capabilities.length > 0
      ? capabilities.map(capability => capability.extension).sort()
      : [...sourceExtensions()].map(ext => ext.slice(1)).sort();
  const factFamilies = [
    ...new Set(capabilities.flatMap(capability => capability.factFamilies)),
  ].sort();
  const sourceFilesByLanguage = countBy(sourceFiles, file => file.language);
  const graphFilesByLanguage = countBy(graphFacts, facts => facts.language);
  const graphByFile = new Set(graphFacts.map(facts => facts.file));
  const missingByExtension = new Map<string, number>();
  for (const file of sourceFiles) {
    if (graphByFile.has(file.rel)) continue;
    missingByExtension.set(
      file.extension,
      (missingByExtension.get(file.extension) ?? 0) + 1
    );
  }
  return {
    graphFactExtensions,
    capabilityCount: capabilities.length,
    factFamilies,
    sourceFilesByLanguage,
    graphFilesByLanguage,
    missingGraphFacts: [...missingByExtension.entries()]
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([extension, files]) => ({
        extension: extension.startsWith('.') ? extension.slice(1) : extension,
        files,
        reason:
          'extension entered the source universe, but native graph facts were unavailable or parser output was empty',
      })),
  };
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

export function buildFileGraph(
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

export function collectEntrypoints(
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
    'src/lib.rs',
    'src/main.rs',
    'lib.rs',
    'main.rs',
    'mod.rs',
  ]) {
    const resolved = resolveExistingPath(path.resolve(root, fallback), known);
    if (resolved) out.add(resolved);
  }
  return [...out];
}

export function reachableFiles(
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
