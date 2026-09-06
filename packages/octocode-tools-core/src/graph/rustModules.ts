import { posix } from 'node:path';
import type { ImportResolution, RawGraphFacts } from './types.js';

interface ModuleContext {
  root: string;
  path: string[];
  file: string;
  scope: string[];
  directory: string;
  attributeDirectory: string;
  depth: number;
}

export type RustDependencies = ReadonlyMap<
  string,
  ReadonlyMap<string, { root?: string; external: boolean }>
>;
const unsupported = (): ImportResolution => ({
  target: null,
  status: 'unsupported',
});
const missing = (): ImportResolution => ({
  target: null,
  status: 'unresolvedInternal',
});
const key = (root: string, path: readonly string[]): string =>
  JSON.stringify([root, path]);
const scopeKey = (file: string, scope: readonly string[]): string =>
  JSON.stringify([file, scope]);

/** Build the logical module forest from declarations; physical neighbors are not modules. */
export function createRustModuleResolver(
  facts: ReadonlyMap<string, RawGraphFacts>,
  files: ReadonlySet<string>,
  roots: readonly string[],
  dependencies: RustDependencies = new Map(),
  editions: ReadonlyMap<string, string> = new Map()
): (
  specifier: string,
  file: string,
  scope?: string[],
  moduleDeclaration?: boolean,
  line?: number
) => ImportResolution {
  const modules = new Map<string, ModuleContext | null>();
  const contexts = new Map<string, ModuleContext[]>();
  const queue: ModuleContext[] = [];
  const maxContexts = Math.max(64, files.size * 8);
  let contextLimit = false;
  const register = (context: ModuleContext): void => {
    const id = key(context.root, context.path);
    if (
      !facts.has(context.file) ||
      facts.get(context.file)?.rustRootUnsupported
    ) {
      modules.set(id, null);
      return;
    }
    if (modules.has(id)) {
      modules.set(id, null);
      return;
    }
    if (context.depth > 64 || queue.length >= maxContexts) {
      modules.set(id, null);
      contextLimit = true;
      return;
    }
    modules.set(id, context);
    const source = scopeKey(context.file, context.scope);
    const values = contexts.get(source) ?? [];
    values.push(context);
    contexts.set(source, values);
    queue.push(context);
  };
  for (const root of new Set(roots)) {
    if (files.has(root))
      register({
        root,
        path: [],
        file: root,
        scope: [],
        directory: posix.dirname(root),
        attributeDirectory: posix.dirname(root),
        depth: 0,
      });
  }
  for (let index = 0; index < queue.length; index++) {
    const owner = queue[index]!;
    if (modules.get(key(owner.root, owner.path)) !== owner) continue;
    for (const declaration of facts.get(owner.file)?.modules ?? []) {
      if (
        scopeKey(owner.file, declaration.scope) !==
        scopeKey(owner.file, owner.scope)
      )
        continue;
      const childPath = [...owner.path, declaration.name];
      const childId = key(owner.root, childPath);
      if (
        declaration.unsupported ||
        (declaration.path !== undefined &&
          posix.isAbsolute(declaration.path)) ||
        declaration.path?.includes('\\') ||
        declaration.path?.includes('\0')
      ) {
        modules.set(childId, null);
        continue;
      }
      if (declaration.inline) {
        const directory =
          declaration.path === undefined
            ? posix.join(owner.directory, declaration.name)
            : posix.normalize(
                posix.join(owner.attributeDirectory, declaration.path)
              );
        register({
          ...owner,
          path: childPath,
          scope: [...owner.scope, declaration.name],
          directory,
          attributeDirectory: directory,
          depth: owner.depth + 1,
        });
        continue;
      }
      const candidates =
        declaration.path === undefined
          ? [
              posix.join(owner.directory, `${declaration.name}.rs`),
              posix.join(owner.directory, declaration.name, 'mod.rs'),
            ]
          : [
              posix.normalize(
                posix.join(owner.attributeDirectory, declaration.path)
              ),
            ];
      const found = candidates.filter(
        candidate =>
          files.has(candidate) &&
          !posix.isAbsolute(candidate) &&
          candidate !== '..' &&
          !candidate.startsWith('../')
      );
      if (found.length !== 1) {
        modules.set(childId, null);
        continue;
      }
      const file = found[0]!;
      const attributeDirectory = posix.dirname(file);
      const directory =
        declaration.path !== undefined || posix.basename(file) === 'mod.rs'
          ? attributeDirectory
          : posix.join(attributeDirectory, declaration.name);
      register({
        root: owner.root,
        path: childPath,
        file,
        scope: [],
        directory,
        attributeDirectory,
        depth: owner.depth + 1,
      });
    }
  }

  return (specifier, file, scope = [], _moduleDeclaration = false) => {
    const parts = specifier.split('::');
    if (parts.some(part => !/^(?:[\p{L}_][\p{L}\p{N}_]*|\*)$/u.test(part)))
      return unsupported();
    const qualifier = parts[0]!;
    const standardLibrary = ['std', 'core', 'alloc'].includes(qualifier);
    const owners = contexts.get(scopeKey(file, scope));
    if (!owners?.length)
      return standardLibrary &&
        !facts.get(file)?.modules?.some(module => module.name === qualifier)
        ? { target: null, status: 'external' }
        : unsupported();
    const outcomes = owners.map(owner => {
      if (modules.get(key(owner.root, owner.path)) !== owner)
        return unsupported();
      let root = owner.root;
      let path = [...owner.path];
      const remaining = [...parts];
      if (qualifier === 'crate') {
        path = [];
        remaining.shift();
      } else if (qualifier === 'self') remaining.shift();
      else if (qualifier === 'super') {
        while (remaining[0] === 'super') {
          if (!path.length) return missing();
          path.pop();
          remaining.shift();
        }
      } else {
        const edition = editions.get(root);
        const modern = ['2018', '2021', '2024'].includes(edition ?? '');
        if (
          path.length > 0 &&
          !modern &&
          edition !== '2015' &&
          (modules.has(key(root, [...path, qualifier])) ||
            modules.has(key(root, [qualifier])))
        )
          return unsupported();
        if (edition === '2015') path = [];
        if (!modules.has(key(root, [...path, qualifier]))) {
          const dependency = dependencies.get(root)?.get(qualifier);
          if (!dependency)
            return standardLibrary
              ? ({ target: null, status: 'external' } as ImportResolution)
              : unsupported();
          if (edition === '2015') return unsupported();
          if (dependency.external)
            return { target: null, status: 'external' } as ImportResolution;
          if (!dependency.root) return unsupported();
          root = dependency.root;
          path = [];
          remaining.shift();
        }
      }
      let current = modules.get(key(root, path));
      if (!current) return unsupported();
      for (let index = 0; index < remaining.length; index++) {
        const part = remaining[index]!;
        if (part === '*' && index === remaining.length - 1) break;
        const childId = key(root, [...path, part]);
        if (!modules.has(childId)) {
          if (index === remaining.length - 1 && !_moduleDeclaration) break;
          return contextLimit ? unsupported() : missing();
        }
        const child = modules.get(childId);
        if (!child) return unsupported();
        current = child;
        path.push(part);
      }
      return { target: current.file, status: 'resolved' } as ImportResolution;
    });
    const first = outcomes[0]!;
    return outcomes.every(
      outcome =>
        outcome.target === first.target && outcome.status === first.status
    )
      ? first
      : unsupported();
  };
}
