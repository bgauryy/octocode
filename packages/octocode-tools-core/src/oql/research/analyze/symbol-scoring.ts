import path from 'node:path';
import { readFile } from 'node:fs/promises';
import type {
  ExportSymbol,
  ResearchRetentionSource,
  ResearchSymbolRow,
  ResearchSymbolVerdict,
  SourceFile,
} from './types.js';
import {
  calleeRefersToSymbol,
  exportKind,
  isRelativeSpecifier,
  resolveImport,
} from './utils.js';

export async function collectExportSymbols(
  sourceFiles: readonly SourceFile[]
): Promise<ExportSymbol[]> {
  const rows = await Promise.all(
    sourceFiles.map(async file => {
      const astExports = exportSymbolsFromGraphFacts(file);
      if (astExports.length > 0) return astExports;
      const text = await readFile(file.path, 'utf8').catch(() => '');
      return exportSymbols(file.rel, text);
    })
  );
  return rows.flat();
}

function exportSymbolsFromGraphFacts(
  file: SourceFile
): readonly ExportSymbol[] {
  const facts = file.graphFacts;
  if (!facts) return [];
  return facts.declarations
    .filter(declaration => declaration.exported)
    .map(declaration => ({
      symbol: declaration.name,
      kind: declaration.kind,
      file: file.rel,
      line: declaration.line,
      evidenceSource: 'ast' as const,
    }));
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
        evidenceSource: 'regex',
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
          symbols.push({
            symbol,
            kind: 'export',
            file,
            line: index + 1,
            evidenceSource: 'regex',
          });
        }
      }
    }
  }
  return symbols;
}

export function scoreSymbols(
  symbols: readonly ExportSymbol[],
  sourceFiles: readonly SourceFile[],
  reachable: ReadonlySet<string>
): readonly ResearchSymbolRow[] {
  const sourceByRel = new Map(sourceFiles.map(file => [file.rel, file]));
  const sourceByPath = new Map(sourceFiles.map(file => [file.path, file]));
  const knownPaths = new Set(sourceFiles.map(file => file.path));
  const exportersBySymbol = buildExporterIndex(sourceFiles);

  return symbols.map(symbol => {
    const declaringFile = sourceByRel.get(symbol.file);
    const astRefs = findAstRetainingFiles(
      symbol,
      sourceFiles,
      declaringFile,
      exportersBySymbol,
      knownPaths,
      sourceByPath
    );
    const retentionSource: ResearchRetentionSource =
      astRefs.length > 0 ? 'ast' : 'ripgrep';
    const refs =
      astRefs.length > 0
        ? astRefs
        : sourceFiles
            .filter(
              file =>
                file.rel !== symbol.file &&
                tokenAppears(file.path, symbol.symbol)
            )
            .map(file => file.rel);
    const reachableRefs = refs.filter(rel => {
      const file = sourceByRel.get(rel);
      return file ? reachable.has(file.path) : false;
    });
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
      evidenceSource: symbol.evidenceSource,
      retentionSource,
      directRefs: refs.length,
      externalRefs: reachableRefs.length,
      retainedBy: refs,
      verdict,
    };
  });
}

function buildExporterIndex(
  sourceFiles: readonly SourceFile[]
): ReadonlyMap<string, ReadonlySet<string>> {
  const index = new Map<string, Set<string>>();
  for (const file of sourceFiles) {
    const facts = file.graphFacts;
    if (!facts) continue;
    for (const decl of facts.declarations) {
      if (!decl.exported) continue;
      const set = index.get(decl.name) ?? new Set<string>();
      set.add(file.path);
      index.set(decl.name, set);
    }
    for (const exp of facts.exports) {
      const set = index.get(exp.name) ?? new Set<string>();
      set.add(file.path);
      index.set(exp.name, set);
    }
  }
  return index;
}

function findAstRetainingFiles(
  symbol: ExportSymbol,
  sourceFiles: readonly SourceFile[],
  declaringFile: SourceFile | undefined,
  exportersBySymbol: ReadonlyMap<string, ReadonlySet<string>>,
  knownPaths: ReadonlySet<string>,
  sourceByPath: ReadonlyMap<string, SourceFile>
): readonly string[] {
  const declaringPaths =
    exportersBySymbol.get(symbol.symbol) ??
    (declaringFile ? new Set([declaringFile.path]) : new Set<string>());
  const retained = new Set<string>();

  for (const file of sourceFiles) {
    if (file.rel === symbol.file) continue;
    const facts = file.graphFacts;
    if (!facts) continue;

    for (const imp of facts.imports) {
      const imported =
        imp.importedName && imp.importedName !== 'default'
          ? imp.importedName
          : undefined;
      const local = imp.localName;
      const namesSymbol = imported === symbol.symbol || local === symbol.symbol;
      if (!namesSymbol) continue;
      if (!isRelativeSpecifier(imp.specifier)) continue;
      const resolved = resolveImport(
        path.dirname(file.path),
        imp.specifier,
        knownPaths
      );
      if (resolved && declaringPaths.has(resolved)) {
        retained.add(file.rel);
        break;
      }
    }

    if (retained.has(file.rel)) continue;

    for (const call of facts.calls) {
      if (!calleeRefersToSymbol(call.callee, symbol.symbol)) continue;
      // Same-file call graph only; still counts as cross-file retention when
      // the call site file is not the declaring export file.
      retained.add(file.rel);
      break;
    }
  }

  // Same-file call retention does not appear in retainedBy (other files only),
  // but import-from-self never happens. sourceByPath kept for future edge work.
  void sourceByPath;
  return [...retained].sort();
}

function tokenAppears(file: string, token: string): boolean {
  return fileContentCache.get(file)?.has(token) ?? false;
}

const fileContentCache = new Map<string, Set<string>>();

export async function cacheTokens(
  sourceFiles: readonly SourceFile[]
): Promise<void> {
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
