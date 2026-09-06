/** Syntax contexts, independent of the host process's own module format. */
export type PackageExportMode =
  'import' | 'require' | 'static' | 'types' | 'types-import' | 'types-require';
type Selection =
  | { kind: 'target'; target: string }
  | { kind: 'unmatched' | 'blocked' | 'invalid' | 'unsupported' };

function invalidPath(value: string): boolean {
  return (
    /[%\\?#]/.test(value) ||
    value
      .split('/')
      .some(
        part =>
          !part ||
          part === '.' ||
          part === '..' ||
          part.toLowerCase() === 'node_modules'
      )
  );
}

function target(
  value: unknown,
  mode: PackageExportMode,
  match: string | undefined,
  budget: { remaining: number },
  depth = 0
): Selection {
  if (--budget.remaining < 0 || depth > 32) return { kind: 'unsupported' };
  if (value === null) return { kind: 'blocked' };
  if (typeof value === 'string') {
    if (!value.startsWith('./') || invalidPath(value.slice(2)))
      return { kind: 'invalid' };
    if (match !== undefined && invalidPath(match)) return { kind: 'invalid' };
    return {
      kind: 'target',
      target: match === undefined ? value : value.replaceAll('*', match),
    };
  }
  if (Array.isArray(value)) {
    if (value.length === 0) return { kind: 'blocked' };
    let last: Selection = { kind: 'unmatched' };
    for (const item of value) {
      const result = target(item, mode, match, budget, depth + 1);
      if (result.kind === 'target' || result.kind === 'unsupported')
        return result;
      if (result.kind !== 'unmatched') last = result;
    }
    return last;
  }
  if (!value || typeof value !== 'object') return { kind: 'invalid' };
  const entries = Object.entries(value);
  if (
    entries.some(([key]) => key.startsWith('.') || /^(0|[1-9]\d*)$/.test(key))
  )
    return { kind: 'invalid' };
  for (const [condition, nested] of entries) {
    const types = mode.startsWith('types');
    const require = mode === 'require' || mode === 'types-require';
    const active =
      condition === 'default' ||
      condition === 'node' ||
      (condition === 'types' && types) ||
      condition === (require ? 'require' : 'import');
    if (active) {
      const result = target(nested, mode, match, budget, depth + 1);
      if (result.kind !== 'unmatched') return result;
    } else if (!['import', 'require', 'types'].includes(condition)) {
      // Custom conditions, Node flags and compiler-version selection are not
      // known from syntax facts. Do not silently choose a competing fallback.
      if (!(condition.startsWith('types@') && !types))
        return { kind: 'unsupported' };
    }
  }
  return { kind: 'unmatched' };
}

/** Node export key order/pattern specificity, with explicit unknown-context gaps. */
export function selectPackageExport(
  exports: unknown,
  subpath: string,
  mode: PackageExportMode
): Selection {
  if (subpath.endsWith('/')) return { kind: 'blocked' };
  if (mode === 'types' || mode === 'static') {
    const imported = selectPackageExport(
      exports,
      subpath,
      mode === 'types' ? 'types-import' : 'import'
    );
    const required = selectPackageExport(
      exports,
      subpath,
      mode === 'types' ? 'types-require' : 'require'
    );
    return JSON.stringify(imported) === JSON.stringify(required)
      ? imported
      : { kind: 'unsupported' };
  }
  let value = exports;
  let match: string | undefined;
  if (exports && typeof exports === 'object' && !Array.isArray(exports)) {
    const entries = Object.entries(exports);
    if (entries.some(([key]) => key.startsWith('.'))) {
      if (entries.some(([key]) => key !== '.' && !key.startsWith('./')))
        return { kind: 'invalid' };
      const exact = entries.find(
        ([key]) => key === subpath && !key.includes('*')
      );
      if (exact) value = exact[1];
      else {
        const patterns = entries
          .filter(([key]) => key.split('*').length === 2)
          .sort(
            ([a], [b]) => b.indexOf('*') - a.indexOf('*') || b.length - a.length
          );
        const selected = patterns.find(([key]) => {
          const [prefix, suffix] = key.split('*') as [string, string];
          return (
            subpath.startsWith(prefix) &&
            subpath.endsWith(suffix) &&
            subpath.length >= key.length
          );
        });
        if (!selected) return { kind: 'blocked' };
        const [key, nested] = selected;
        const star = key.indexOf('*');
        match = subpath.slice(star, subpath.length - (key.length - star - 1));
        value = nested;
      }
    } else if (subpath !== '.') return { kind: 'blocked' };
  } else if (subpath !== '.') return { kind: 'blocked' };
  return target(value, mode, match, { remaining: 2048 });
}
