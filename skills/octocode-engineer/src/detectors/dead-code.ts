import { findImportLine, isLikelyEntrypoint } from './shared.js';
import { isTestFile } from '../common/utils.js';

import type { FindingDraft } from './shared.js';
import type { DependencyState } from '../types/index.js';

export function buildConsumedFromModule(dependencyState: DependencyState): {
  production: Map<string, Set<string>>;
  test: Map<string, Set<string>>;
} {
  const production = new Map<string, Set<string>>();
  const test = new Map<string, Set<string>>();
  for (const [
    file,
    imports,
  ] of dependencyState.importedSymbolsByFile.entries()) {
    const targetMap = isTestFile(file) ? test : production;
    for (const symbol of imports) {
      const target = symbol.resolvedModule;
      if (!target) continue;
      if (!targetMap.has(target)) targetMap.set(target, new Set());
      targetMap.get(target)!.add(symbol.importedName);
    }
  }
  for (const [file, reexports] of dependencyState.reExportsByFile.entries()) {
    const targetMap = isTestFile(file) ? test : production;
    for (const reexport of reexports) {
      const target = reexport.resolvedModule;
      if (!target) continue;
      if (!targetMap.has(target)) targetMap.set(target, new Set());
      targetMap.get(target)!.add(reexport.importedName);
    }
  }
  return { production, test };
}

export function detectDeadExports(
  dependencyState: DependencyState,
  consumedFromModule: Map<string, Set<string>>,
  testConsumedFromModule?: Map<string, Set<string>>
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const [
    file,
    exportsList,
  ] of dependencyState.declaredExportsByFile.entries()) {
    if (isTestFile(file)) continue;
    if (isLikelyEntrypoint(file)) continue;
    const consumed = consumedFromModule.get(file) || new Set<string>();
    const testConsumed = testConsumedFromModule?.get(file) || new Set<string>();
    const hasNamespaceUse = consumed.has('*');
    const hasTestNamespaceUse = testConsumed.has('*');
    for (const exported of exportsList) {
      if (exported.name === 'default' && isLikelyEntrypoint(file)) continue;
      if (hasNamespaceUse || consumed.has(exported.name)) continue;
      if (hasTestNamespaceUse || testConsumed.has(exported.name)) continue;
      findings.push({
        severity: exported.kind === 'type' ? 'medium' : 'high',
        category: 'dead-export',
        file,
        lineStart: exported.lineStart || 1,
        lineEnd: exported.lineEnd || exported.lineStart || 1,
        title: `Unused export: ${exported.name}`,
        reason: `Exported symbol "${exported.name}" has no observed import or re-export usage in production or test files.`,
        files: [
          `${file}:${exported.lineStart || 1}-${exported.lineEnd || exported.lineStart || 1}`,
        ],
        suggestedFix: {
          strategy: 'Remove or internalize unused exports.',
          steps: [
            'Confirm symbol is not part of intentional public API surface.',
            'Remove export modifier or delete symbol if truly unused.',
            'Re-run scan and tests to ensure no hidden runtime usage.',
          ],
        },
        impact: 'Shrinks public API surface and reduces accidental coupling.',
        tags: ['dead-code', 'api-surface', 'cleanup'],
        lspHints: [
          {
            tool: 'lspGetSemantics', semanticType: 'references',
            symbolName: exported.name,
            lineHint: exported.lineStart || 1,
            file,
            expectedResult: `confirm "${exported.name}" has no import references before removing`,
          },
        ],
      });
    }
  }
  return findings;
}

export function detectDeadReExports(
  dependencyState: DependencyState,
  consumedFromModule: Map<string, Set<string>>
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const [
    barrelFile,
    reexports,
  ] of dependencyState.reExportsByFile.entries()) {
    if (isTestFile(barrelFile)) continue;
    const consumed = consumedFromModule.get(barrelFile) || new Set<string>();
    const hasNamespaceUse = consumed.has('*');
    const sourceByExportedAs = new Map<string, Set<string>>();
    const localExportNames = new Set(
      (dependencyState.declaredExportsByFile.get(barrelFile) || []).map(
        entry => entry.name
      )
    );

    for (const ref of reexports) {
      const exportedAs = ref.exportedAs;
      if (!sourceByExportedAs.has(exportedAs))
        sourceByExportedAs.set(exportedAs, new Set());
      sourceByExportedAs
        .get(exportedAs)!
        .add(ref.resolvedModule || ref.sourceModule);

      const isUsed =
        hasNamespaceUse ||
        consumed.has(exportedAs) ||
        (ref.isStar && consumed.size > 0);
      if (!isUsed) {
        findings.push({
          severity: 'medium',
          category: 'dead-re-export',
          file: barrelFile,
          lineStart: ref.lineStart || 1,
          lineEnd: ref.lineEnd || ref.lineStart || 1,
          title: `Unused re-export: ${exportedAs}`,
          reason: `Re-exported symbol "${exportedAs}" from ${ref.sourceModule} has no observed downstream imports from this module.`,
          files: [
            `${barrelFile}:${ref.lineStart || 1}-${ref.lineEnd || ref.lineStart || 1}`,
          ],
          suggestedFix: {
            strategy: 'Remove stale barrel re-exports.',
            steps: [
              'Verify no dynamic import/runtime reflection depends on this export.',
              'Remove the re-export clause.',
              'Re-run scan to confirm barrel surface is still complete.',
            ],
          },
          impact: 'Keeps barrel modules focused and easier to reason about.',
          tags: ['dead-code', 'barrel', 'cleanup'],
        });
      }
    }

    for (const [name, sources] of sourceByExportedAs.entries()) {
      if (sources.size > 1) {
        findings.push({
          severity: 'medium',
          category: 're-export-duplication',
          file: barrelFile,
          lineStart: 1,
          lineEnd: 1,
          title: `Duplicate re-export paths: ${name}`,
          reason: `Symbol "${name}" is re-exported from multiple sources in the same barrel.`,
          files: [barrelFile],
          suggestedFix: {
            strategy: 'Keep one canonical re-export source per symbol.',
            steps: [
              'Select a canonical module for the symbol.',
              'Remove duplicate re-export paths.',
              'Document intended public export map for the barrel.',
            ],
          },
          impact: 'Reduces API ambiguity and import inconsistency.',
          tags: ['duplication', 'barrel', 'api-surface'],
        });
      }
      if (name !== '*' && localExportNames.has(name)) {
        findings.push({
          severity: 'high',
          category: 're-export-shadowed',
          file: barrelFile,
          lineStart: 1,
          lineEnd: 1,
          title: `Shadowed export in barrel: ${name}`,
          reason: `Barrel exports "${name}" both locally and through re-export, which can hide origin and create ambiguity.`,
          files: [barrelFile],
          suggestedFix: {
            strategy: 'Disambiguate local vs re-exported symbol ownership.',
            steps: [
              'Pick a single source of truth for the symbol in this barrel.',
              'Rename or remove the conflicting export path.',
              'Update import call-sites to use the canonical export.',
            ],
          },
          impact: 'Prevents subtle API conflicts and shadowing confusion.',
          tags: ['barrel', 'api-surface', 'ambiguity'],
        });
      }
    }
  }
  return findings;
}

export function detectUnusedNpmDeps(
  externalDeps: Map<string, Set<string>>,
  packageJsonDeps: Record<string, string>,
  devDeps: Record<string, string> = {}
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  const usedPackages = new Set<string>();
  for (const depSet of externalDeps.values()) {
    for (const dep of depSet) {
      const parts = dep.split('/');
      usedPackages.add(
        dep.startsWith('@') && parts.length >= 2
          ? `${parts[0]}/${parts[1]}`
          : parts[0]
      );
    }
  }

  for (const pkgName of Object.keys(packageJsonDeps)) {
    if (!usedPackages.has(pkgName)) {
      findings.push({
        severity: 'medium',
        category: 'unused-npm-dependency',
        file: 'package.json',
        lineStart: 1,
        lineEnd: 1,
        title: `Unused dependency: ${pkgName}`,
        reason: `Package "${pkgName}" is in dependencies but no import was found.`,
        files: ['package.json'],
        suggestedFix: {
          strategy: 'Remove unused dependency from package.json.',
          steps: [
            'Verify the package is not loaded dynamically or via CLI scripts.',
            'Check if it is a peer dependency required at runtime.',
            'Run `npm uninstall` or remove from package.json.',
          ],
        },
        impact: 'Reduces install size and attack surface.',
        tags: ['dependency', 'hygiene', 'bundle-size'],
      });
    }
  }

  for (const pkgName of Object.keys(devDeps)) {
    if (!usedPackages.has(pkgName)) {
      findings.push({
        severity: 'low',
        category: 'unused-npm-dependency',
        file: 'package.json',
        lineStart: 1,
        lineEnd: 1,
        title: `Unused devDependency: ${pkgName}`,
        reason: `Package "${pkgName}" is in devDependencies but no import was found.`,
        files: ['package.json'],
        suggestedFix: {
          strategy: 'Remove unused devDependency from package.json.',
          steps: [
            'Verify the package is not used by build scripts, config files, or CLI tools.',
            'Run `npm uninstall` or remove from package.json.',
          ],
        },
        impact: 'Reduces install size and dependency maintenance burden.',
        tags: ['dependency', 'hygiene', 'dev-tooling'],
      });
    }
  }

  return findings;
}

export function detectBoundaryViolations(
  dependencyState: DependencyState
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  for (const file of dependencyState.files) {
    if (isTestFile(file)) continue;

    const fileMatch = file.match(/^packages\/([^/]+)\//);
    if (!fileMatch) continue;
    const filePkg = fileMatch[1];

    for (const dep of dependencyState.outgoing.get(file) || new Set()) {
      const depMatch = dep.match(/^packages\/([^/]+)\//);
      if (!depMatch) continue;
      if (depMatch[1] === filePkg) continue;

      const isPublicApi = /^packages\/[^/]+\/(src\/)?index\.[mc]?[jt]sx?$/.test(
        dep
      );
      if (!isPublicApi) {
        const isDeep = dep.includes('/internal/') || dep.includes('/private/');
        const importRef = findImportLine(dependencyState, file, dep);
        findings.push({
          severity: isDeep ? 'high' : 'medium',
          category: 'package-boundary-violation',
          file,
          lineStart: importRef.lineStart,
          lineEnd: importRef.lineEnd,
          title: `Cross-package import bypasses public API`,
          reason: `"${file}" imports "${dep}" directly instead of through the package public entry.`,
          files: [file, dep],
          suggestedFix: {
            strategy: 'Import through the package public API (index file).',
            steps: [
              'Re-export the needed symbol from the target package index.',
              'Update the import to use the package name or index path.',
              'If the symbol is internal, reconsider the dependency.',
            ],
          },
          impact:
            'Enforces clean package boundaries and prevents coupling to internals.',
          tags: ['boundary', 'coupling', 'encapsulation'],
        });
      }
    }
  }

  return findings;
}
