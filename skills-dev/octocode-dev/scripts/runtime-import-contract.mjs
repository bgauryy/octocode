import { builtinModules } from 'node:module';

const nodeBuiltins = new Set([
  ...builtinModules,
  ...builtinModules.map(moduleName => `node:${moduleName}`),
]);

function packageName(specifier) {
  const parts = specifier.split('/');
  return specifier.startsWith('@') ? parts.slice(0, 2).join('/') : parts[0];
}

export function undeclaredExternalRuntimeImports(metafiles, dependencies = {}) {
  const declared = new Set(Object.keys(dependencies));
  const externalPackages = new Set();

  for (const metafile of metafiles) {
    for (const output of Object.values(metafile.outputs)) {
      for (const imported of output.imports) {
        if (!imported.external) continue;
        const specifier = imported.path;
        if (
          specifier.startsWith('.') ||
          specifier.startsWith('/') ||
          nodeBuiltins.has(specifier)
        ) {
          continue;
        }
        externalPackages.add(packageName(specifier));
      }
    }
  }

  return [...externalPackages].filter(name => !declared.has(name)).sort();
}

export function assertDeclaredRuntimeImports({
  metafiles,
  dependencies,
  label = 'bundle',
}) {
  const undeclared = undeclaredExternalRuntimeImports(metafiles, dependencies);
  if (undeclared.length > 0) {
    throw new Error(
      `${label} has undeclared external runtime import(s): ${undeclared.join(', ')}`
    );
  }
}
