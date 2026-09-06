import { posix } from 'node:path';
import {
  createRustModuleResolver,
  type RustDependencies,
} from './rustModules.js';
import { resolveRustCargoMetadata } from './rustCargoMetadata.js';
import type { RawGraphFacts, GraphCoverage } from './types.js';

export async function prepareRustResolver(
  rootAbsolutePath: string,
  knownFiles: ReadonlySet<string>,
  parsedEntries: Array<{ relativePath: string; parsed: RawGraphFacts }>,
  rustWorkspace: 'syntax' | 'cargo',
  coverage: GraphCoverage
) {
  const rustFacts = new Map(
    parsedEntries
      .filter(entry => entry.relativePath.endsWith('.rs'))
      .map(entry => [entry.relativePath, entry.parsed])
  );
  let rustRoots = [...rustFacts.keys()].filter(
    file =>
      ['lib.rs', 'main.rs'].includes(posix.basename(file)) &&
      !/(^|\/)(bin|tests|benches|examples)\//.test(file)
  );
  let rustDependencies: RustDependencies = new Map();
  let rustEditions: ReadonlyMap<string, string> = new Map();
  if (rustWorkspace === 'cargo' && rustFacts.size > 0) {
    const metadata = await resolveRustCargoMetadata({
      root: rootAbsolutePath,
      files: [...knownFiles],
    });
    const fingerprints = new Map<string, string>();
    const ambiguousRoots = new Set<string>();
    for (const target of metadata.targets) {
      const fingerprint = JSON.stringify([
        target.edition,
        target.dependencyAliases
          .map(dependency => [
            dependency.alias,
            dependency.targetId,
            dependency.external,
            dependency.conditional,
          ])
          .sort((a, b) => String(a[0]).localeCompare(String(b[0]))),
      ]);
      if (
        fingerprints.has(target.srcPath) &&
        fingerprints.get(target.srcPath) !== fingerprint
      )
        ambiguousRoots.add(target.srcPath);
      fingerprints.set(target.srcPath, fingerprint);
    }
    const targets = metadata.targets.filter(
      target => !ambiguousRoots.has(target.srcPath)
    );
    rustRoots = targets.map(target => target.srcPath);
    rustEditions = new Map(
      targets
        .filter(target => target.edition !== undefined)
        .map(target => [target.srcPath, target.edition!])
    );
    for (const file of ambiguousRoots)
      coverage.diagnostics.push({
        file,
        code: 'unsupported-linking',
        message:
          'Cargo targets share a source file but disagree on edition or dependency context; root resolution is ambiguous.',
      });
    const rootsById = new Map(
      metadata.targets.map(target => [target.id, target.srcPath])
    );
    rustDependencies = new Map(
      targets.map(target => {
        const aliases = new Map<string, { root?: string; external: boolean }>();
        const identities = new Map<string, string>();
        for (const dependency of target.dependencyAliases) {
          const identity = JSON.stringify([
            dependency.packageName,
            dependency.targetId,
            dependency.external,
            dependency.conditional,
          ]);
          if (
            identities.has(dependency.alias) &&
            identities.get(dependency.alias) !== identity
          ) {
            aliases.set(dependency.alias, { external: false });
            // Keep an ambiguity sentinel so later entries cannot restore a guessed edge.
            identities.set(dependency.alias, 'ambiguous');
            coverage.diagnostics.push({
              file: target.srcPath,
              code: 'unsupported-linking',
              message: `Cargo dependency alias ${dependency.alias} has conflicting contexts; import resolution is ambiguous.`,
            });
          } else {
            identities.set(dependency.alias, identity);
            aliases.set(dependency.alias, {
              root: dependency.conditional
                ? undefined
                : rootsById.get(dependency.targetId ?? ''),
              external: !dependency.conditional && dependency.external,
            });
          }
        }
        return [target.srcPath, aliases];
      })
    );
    for (const diagnostic of metadata.diagnostics)
      coverage.diagnostics.push({
        file: '.',
        code: 'unsupported-linking',
        message: `Cargo metadata ${diagnostic.code}: ${diagnostic.message}`,
      });
  }
  return createRustModuleResolver(
    rustFacts,
    knownFiles,
    rustRoots,
    rustDependencies,
    rustEditions
  );
}
