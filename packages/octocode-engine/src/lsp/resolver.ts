import { access, constants, readFile } from 'node:fs/promises';
import path from 'node:path';

import { nativeBinding } from './native.js';
import type { CodeSnippet, ExactPosition, FuzzyPosition } from './types.js';

export class SymbolResolutionError extends Error {
  constructor(
    public readonly symbolName: string,
    public readonly lineHint: number,
    public readonly reason: string,
    public readonly searchRadius = 5
  ) {
    super(
      `Could not find symbol '${symbolName}' at or near line ${lineHint}. ${reason}`
    );
    this.name = 'SymbolResolutionError';
  }
}

interface SymbolResolverConfig {
  lineSearchRadius?: number;
}

interface ResolvedSymbol {
  position: ExactPosition;
  foundAtLine: number;
  lineOffset: number;
  lineContent: string;
}

export interface ImportAliasDefinitionInput {
  anchorUri: string;
  symbolName: string;
  locations: CodeSnippet[];
}

function normalizeResolvedSymbol(value: unknown): ResolvedSymbol {
  const record = value as {
    position: ExactPosition;
    foundAtLine?: number;
    found_at_line?: number;
    lineOffset?: number;
    line_offset?: number;
    lineContent?: string;
    line_content?: string;
  };
  return {
    position: record.position,
    foundAtLine: record.foundAtLine ?? record.found_at_line ?? 0,
    lineOffset: record.lineOffset ?? record.line_offset ?? 0,
    lineContent: record.lineContent ?? record.line_content ?? '',
  };
}

function toSymbolResolutionError(
  error: unknown,
  fuzzy: FuzzyPosition,
  searchRadius = 5
): SymbolResolutionError {
  if (error instanceof SymbolResolutionError) return error;
  const reason = error instanceof Error ? error.message : String(error);
  return new SymbolResolutionError(
    fuzzy.symbolName,
    fuzzy.lineHint ?? 0,
    reason,
    searchRadius
  );
}

// Overload 1 (async): resolves a symbol position by reading from disk.
// Overload 2 (sync): resolves from a content string already in memory.
// Discriminant: second arg is `string` (symbol name) → async/file path;
//               second arg is `FuzzyPosition` object → sync/content.
// Note: this function is internal (not re-exported from index.ts). External
// callers should use SymbolResolver.resolvePositionFromContent (sync) or
// SymbolResolver.resolvePosition (async via file path).
export async function resolveSymbolPosition(
  filePath: string,
  symbolName: string,
  lineHint?: number,
  orderHint?: number
): Promise<ResolvedSymbol>;
export function resolveSymbolPosition(
  content: string,
  fuzzy: FuzzyPosition
): ResolvedSymbol;
export function resolveSymbolPosition(
  fileOrContent: string,
  fuzzyOrSymbolName: FuzzyPosition | string,
  lineHint?: number,
  orderHint?: number
): Promise<ResolvedSymbol> | ResolvedSymbol {
  if (typeof fuzzyOrSymbolName === 'string') {
    const fuzzy = {
      symbolName: fuzzyOrSymbolName,
      lineHint,
      orderHint,
    };
    try {
      return Promise.resolve(
        normalizeResolvedSymbol(
          nativeBinding.resolvePosition(fileOrContent, fuzzy)
        )
      );
    } catch (error) {
      return Promise.reject(toSymbolResolutionError(error, fuzzy));
    }
  }
  try {
    return normalizeResolvedSymbol(
      nativeBinding.resolvePositionFromContent(fileOrContent, fuzzyOrSymbolName)
    );
  } catch (error) {
    throw toSymbolResolutionError(error, fuzzyOrSymbolName);
  }
}

export class SymbolResolver {
  readonly lineSearchRadius: number;

  constructor(config?: SymbolResolverConfig) {
    this.lineSearchRadius = config?.lineSearchRadius ?? 5;
  }

  async resolvePosition(
    filePath: string,
    fuzzy: FuzzyPosition
  ): Promise<ResolvedSymbol> {
    try {
      return normalizeResolvedSymbol(
        nativeBinding.resolvePosition(filePath, fuzzy)
      );
    } catch (error) {
      throw toSymbolResolutionError(error, fuzzy, this.lineSearchRadius);
    }
  }

  resolvePositionFromContent(
    content: string,
    fuzzy: FuzzyPosition
  ): ResolvedSymbol {
    try {
      return normalizeResolvedSymbol(
        nativeBinding.resolvePositionFromContent(content, fuzzy)
      );
    } catch (error) {
      throw toSymbolResolutionError(error, fuzzy, this.lineSearchRadius);
    }
  }
}

export async function resolveImportAliasDefinitions({
  anchorUri,
  symbolName,
  locations,
}: ImportAliasDefinitionInput): Promise<CodeSnippet[]> {
  const resolved = await Promise.all(
    locations.map(location =>
      resolveImportAliasDefinition(anchorUri, symbolName, location)
    )
  );
  return resolved;
}

async function resolveImportAliasDefinition(
  anchorUri: string,
  symbolName: string,
  location: CodeSnippet
): Promise<CodeSnippet> {
  const locationPath = snippetPath(location.uri, anchorUri);
  if (!isSamePath(locationPath, anchorUri)) return location;
  if (!isImportSnippet(location.content)) return location;

  const importTarget = importTargetForSymbol(location.content, symbolName);
  if (!importTarget?.moduleSpecifier.startsWith('.')) return location;

  const targetPath = await resolveLocalModulePath(
    locationPath,
    importTarget.moduleSpecifier
  );
  if (!targetPath) return location;

  const content = await readFile(targetPath, 'utf-8');
  const declaration = findExportedDeclaration(
    content,
    importTarget.exportedName
  );
  if (!declaration) return location;

  return {
    uri: targetPath,
    range: {
      start: { line: declaration.line - 1, character: declaration.character },
      end: { line: declaration.line - 1, character: declaration.character },
    },
    displayRange: { startLine: declaration.line, endLine: declaration.line },
    content: declaration.content,
  };
}

function isImportSnippet(content: string): boolean {
  return /^\s*import\s/.test(content.trim());
}

function importTargetForSymbol(
  importLine: string,
  symbolName: string
): { moduleSpecifier: string; exportedName: string } | undefined {
  const namedImport = importLine.match(
    /import\s*\{([^}]+)\}\s*from\s*['"]([^'"]+)['"]/
  );
  const namedImports = namedImport?.[1];
  const namedModulePath = namedImport?.[2];
  if (namedImports && namedModulePath) {
    for (const part of namedImports.split(',')) {
      const [original, alias] = part
        .trim()
        .split(/\s+as\s+/)
        .map(value => value.trim());
      if (alias === symbolName || original === symbolName) {
        return {
          moduleSpecifier: namedModulePath,
          exportedName: original,
        };
      }
    }
  }

  const defaultImport = importLine.match(
    /import\s+([A-Za-z_$][\w$]*)\s+from\s*['"]([^'"]+)['"]/
  );
  const defaultName = defaultImport?.[1];
  const defaultModulePath = defaultImport?.[2];
  if (defaultName === symbolName && defaultModulePath) {
    return { moduleSpecifier: defaultModulePath, exportedName: symbolName };
  }

  return undefined;
}

async function resolveLocalModulePath(
  importerPath: string,
  moduleSpecifier: string
): Promise<string | undefined> {
  const basePath = path.resolve(
    path.dirname(filePathFromUri(importerPath)),
    moduleSpecifier
  );
  const extension = path.extname(basePath);
  const sourcePath = extension ? basePath.slice(0, -extension.length) : basePath;
  const candidates = [
    ...(extension === '.js' || extension === '.jsx'
      ? [`${sourcePath}.ts`, `${sourcePath}.tsx`]
      : []),
    basePath,
    `${basePath}.ts`,
    `${basePath}.tsx`,
    `${basePath}.js`,
    `${basePath}.jsx`,
    path.join(basePath, 'index.ts'),
    path.join(basePath, 'index.tsx'),
    path.join(basePath, 'index.js'),
    path.join(basePath, 'index.jsx'),
  ];

  for (const candidate of candidates) {
    try {
      await access(candidate, constants.R_OK);
      return candidate;
    } catch {
      // Try the next TypeScript/JavaScript resolution candidate.
    }
  }

  return undefined;
}

function findExportedDeclaration(
  content: string,
  symbolName: string
): { line: number; character: number; content: string } | undefined {
  const escaped = symbolName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const declarationPattern = new RegExp(
    `^\\s*export\\s+(?:default\\s+)?(?:async\\s+)?(?:function|class|interface|type|const|let|var|enum)\\s+${escaped}\\b`
  );
  const lines = content.split(/\r?\n/);
  for (const [index, line] of lines.entries()) {
    if (!declarationPattern.test(line)) continue;
    return {
      line: index + 1,
      character: Math.max(0, line.indexOf(symbolName)),
      content: line.trim(),
    };
  }
  return undefined;
}

function isSamePath(left: string, right: string): boolean {
  return (
    path.resolve(filePathFromUri(left)) === path.resolve(filePathFromUri(right))
  );
}

function snippetPath(uri: string, anchorUri: string): string {
  const filePath = filePathFromUri(uri);
  if (path.isAbsolute(filePath)) return filePath;
  return path.resolve(path.dirname(filePathFromUri(anchorUri)), filePath);
}

function filePathFromUri(uri: string): string {
  return uri.startsWith('file://') ? new URL(uri).pathname : uri;
}
