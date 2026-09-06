import { posix } from 'node:path';
import type { ImportResolution } from './types.js';

const unsupported: ImportResolution = { target: null, status: 'unsupported' };

/** Resolve exact syntax paths only; build-system search paths remain unsupported. */
export function resolveCInclude(
  specifier: string,
  importer: string,
  hint: string | undefined,
  files: ReadonlySet<string>
): ImportResolution {
  if (hint !== 'c-relative' || posix.isAbsolute(specifier)) return unsupported;
  const target = posix.normalize(
    posix.join(posix.dirname(importer), specifier)
  );
  if (target === '..' || target.startsWith('../')) return unsupported;
  return files.has(target)
    ? { target, status: 'resolved' }
    : { target: null, status: 'unresolvedInternal' };
}

/** The selected root is a candidate Python import root, not a sys.path claim. */
export function resolvePythonImport(
  specifier: string,
  importer: string,
  importedName: string | undefined,
  hint: string | undefined,
  files: ReadonlySet<string>
): ImportResolution {
  if (hint !== 'python-relative' && hint !== 'python-absolute')
    return unsupported;
  let dots = 0;
  while (specifier[dots] === '.') dots++;
  let base = dots ? posix.dirname(importer) : '.';
  const additionalTargets = new Set<string>();
  // Every relative step needs a unique known package. The selected root may
  // itself be a package, but no ascent may escape it or cross a namespace gap.
  for (let i = 0; i < dots; i++) {
    const initializers = pythonInitializers(base, files);
    if (initializers.length !== 1) return unsupported;
    if (initializers[0] !== importer) additionalTargets.add(initializers[0]!);
    if (i + 1 < dots) {
      if (base === '.' || base === '..' || base.startsWith('../'))
        return unsupported;
      base = posix.dirname(base);
    }
  }
  const module = specifier.slice(dots).split('.').filter(Boolean).join('/');
  let stem = posix.normalize(posix.join(base, module));
  if (posix.isAbsolute(stem) || stem === '..' || stem.startsWith('../'))
    return unsupported;
  const candidates =
    !module && dots
      ? pythonInitializers(stem, files)
      : pythonModuleCandidates(stem, files);
  // These interpreter modules precede filesystem search under Python's default
  // import machinery. A colliding local file cannot establish a target; custom
  // import hooks and the wider platform-dependent builtin set remain unmodelled.
  if (!dots && ['sys', 'builtins'].includes(module) && candidates.length)
    return unsupported;
  if (candidates.length > 1) return unsupported;
  if (!candidates.length) {
    const prefix = `${stem}/`;
    const namespace = [...files].some(file => file.startsWith(prefix));
    return {
      target: null,
      status: namespace
        ? 'unsupported'
        : dots
          ? 'unresolvedInternal'
          : 'external',
    };
  }
  let target = candidates[0]!;
  additionalTargets.delete(target);
  let status: ImportResolution['status'] = 'resolved';
  if (
    ['__init__.py', '__init__.pyi'].includes(posix.basename(target)) &&
    importedName
  ) {
    const submodules =
      importedName === '*'
        ? []
        : pythonModuleCandidates(
            posix.join(posix.dirname(target), importedName),
            files
          );
    // The initializer is a known dependency even when the imported name may
    // instead be a package attribute or an ambiguous module layout.
    if (submodules.length !== 1) {
      if (importedName !== '*' || dots) status = 'unsupported';
    } else {
      if (target !== importer) additionalTargets.add(target);
      target = submodules[0]!;
      // Namespace retention is distinct from the deduplicated file edge.
      if (target !== importer) additionalTargets.add(target);
    }
  }
  stem = posix.dirname(target);
  while (stem !== '.') {
    const initializers = pythonInitializers(stem, files);
    if (initializers.length > 1) status = 'unsupported';
    for (const initializer of initializers) {
      if (initializer !== target && initializer !== importer)
        additionalTargets.add(initializer);
    }
    stem = posix.dirname(stem);
  }
  return {
    target: target === importer ? null : target,
    status,
    additionalTargets: [...additionalTargets].sort(),
  };
}

function pythonInitializers(
  directory: string,
  files: ReadonlySet<string>
): string[] {
  return ['__init__.py', '__init__.pyi']
    .map(name => posix.join(directory, name))
    .filter(file => files.has(file));
}

function pythonModuleCandidates(
  stem: string,
  files: ReadonlySet<string>
): string[] {
  return [
    `${stem}.py`,
    `${stem}.pyi`,
    ...pythonInitializers(stem, files),
  ].filter(file => files.has(file));
}
