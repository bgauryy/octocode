import { posix } from 'node:path';
import { recordUnlinkedCommonJsCalls } from './coverage.js';
import { resolveImportSpecifier } from './importResolver.js';
import {
  resolveCInclude,
  resolvePythonImport,
} from './languageImportResolver.js';
import type { prepareMetadataImports } from './metadataImports.js';
import type { prepareRustResolver } from './rustWorkspace.js';
import type { WorkspacePackageExports } from './workspacePackageResolver.js';
import type { PackageExportMode } from './packageExportTargets.js';
import type {
  GraphCoverage,
  ImportResolution,
  RawGraphFacts,
} from './types.js';

/** One coverage policy for every source import, reexport, and module load. */
export function createFileImportLinker(options: {
  relativePath: string;
  parsed: RawGraphFacts;
  knownFiles: Set<string>;
  getWorkspacePackageExports: () => WorkspacePackageExports;
  metadata: ReturnType<typeof prepareMetadataImports>;
  resolveRust: Awaited<ReturnType<typeof prepareRustResolver>>;
  coverage: GraphCoverage;
}) {
  const {
    relativePath,
    parsed,
    knownFiles,
    getWorkspacePackageExports,
    metadata,
    resolveRust,
    coverage,
  } = options;
  const extension = posix.extname(relativePath).slice(1);
  const language = parsed.language ?? extension;
  const javascript = [
    'js',
    'jsx',
    'ts',
    'tsx',
    'mjs',
    'cjs',
    'mts',
    'cts',
  ].includes(extension);
  const python = ['py', 'pyi'].includes(extension);
  const cFamily = ['c', 'h', 'cc', 'cpp', 'cxx', 'hh', 'hpp', 'hxx'].includes(
    extension
  );
  const linking = javascript
    ? 'javascript-relative'
    : extension === 'rs'
      ? 'rust-modules'
      : python
        ? 'python-modules'
        : cFamily
          ? 'c-relative-includes'
          : 'unsupported';
  const languageCoverage = coverage.languages.find(
    item => item.language === language
  );
  if (languageCoverage) languageCoverage.files++;
  else coverage.languages.push({ language, files: 1, linking });
  for (const message of parsed.diagnostics ?? []) {
    coverage.diagnostics.push({
      file: relativePath,
      code: message.startsWith('unsupported ')
        ? 'unsupported-linking'
        : message.startsWith('tree-sitter graph facts are syntax-only;')
          ? 'syntax-only'
          : 'parse-recovery',
      message,
    });
  }
  if (linking === 'unsupported') {
    coverage.diagnostics.push({
      file: relativePath,
      code: 'unsupported-linking',
      message: `Import linking is unsupported for ${language}; declarations are syntax facts only.`,
    });
  }
  if (javascript && !parsed.commonJs)
    recordUnlinkedCommonJsCalls(coverage, relativePath, parsed.calls);
  const auxiliaryTargets = new Set<string>();
  const resolve = (
    specifier: string,
    line: number,
    options: {
      moduleDeclaration?: boolean;
      resolutionHint?: string;
      moduleScope?: string[];
      importedName?: string;
      exportMode?: PackageExportMode;
    } = {}
  ): string | null => {
    const {
      moduleDeclaration = false,
      resolutionHint,
      moduleScope,
      importedName,
      exportMode = 'import',
    } = options;
    const mode =
      exportMode === 'types'
        ? ['mts', 'mjs'].includes(extension)
          ? 'types-import'
          : ['cts', 'cjs'].includes(extension)
            ? 'types-require'
            : 'types'
        : exportMode === 'static'
          ? extension === 'cts'
            ? 'require'
            : ['ts', 'tsx'].includes(extension)
              ? 'static'
              : 'import'
          : exportMode;
    let resolution: ImportResolution;
    if (
      resolutionHint === 'unsupported' ||
      linking === 'unsupported' ||
      (javascript && specifier.startsWith('/'))
    )
      resolution = { target: null, status: 'unsupported' };
    else if (extension === 'rs')
      resolution = resolveRust(
        specifier,
        relativePath,
        moduleScope,
        moduleDeclaration,
        line
      );
    else if (python)
      resolution = resolvePythonImport(
        specifier,
        relativePath,
        importedName,
        resolutionHint,
        knownFiles
      );
    else if (cFamily)
      resolution = resolveCInclude(
        specifier,
        relativePath,
        resolutionHint,
        knownFiles
      );
    else {
      const metadataResolution = metadata.resolve(specifier, relativePath);
      const target = resolveImportSpecifier(
        specifier,
        relativePath,
        knownFiles
      );
      const workspaceResolution =
        !specifier.startsWith('.') && !specifier.startsWith('/')
          ? getWorkspacePackageExports().resolve(specifier, mode)
          : undefined;
      resolution = metadataResolution ??
        workspaceResolution ?? {
          target,
          status: target
            ? 'resolved'
            : specifier.startsWith('.') || specifier.startsWith('/')
              ? 'unresolvedInternal'
              : 'external',
        };
    }
    for (const target of resolution.additionalTargets ?? [])
      auxiliaryTargets.add(target);
    coverage.imports[resolution.status]++;
    if (
      resolution.status === 'unsupported' ||
      resolution.status === 'unresolvedInternal'
    ) {
      coverage.diagnostics.push({
        file: relativePath,
        line,
        code:
          resolution.status === 'unsupported'
            ? 'unsupported-linking'
            : 'unresolved-internal',
        message: `Cannot link import ${JSON.stringify(specifier)} (${resolution.status}${resolution.reason ? `: ${resolution.reason}` : ''}).`,
      });
    }
    return resolution.target;
  };
  return { extension, javascript, python, cFamily, auxiliaryTargets, resolve };
}
