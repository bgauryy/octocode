import { lstatSync, readFileSync, realpathSync } from 'node:fs';
import {
  basename,
  dirname,
  isAbsolute,
  relative,
  resolve,
  sep,
} from 'node:path';
import type { ImportResolution } from './types.js';

const MAX_MANIFEST_BYTES = 64 * 1024;

/** Only explicit package.json imports become leaves; never traverse their contents. */
export function prepareMetadataImports(
  root: string,
  knownFiles: Set<string>,
  excludeDirs: readonly string[],
  maxFiles: number
): {
  targets: Set<string>;
  resolve: (
    specifier: string,
    importer: string
  ) => ImportResolution | undefined;
  truncated: () => boolean;
} {
  const targets = new Set<string>();
  const cache = new Map<string, ImportResolution>();
  let realRoot = resolve(root);
  let initialized = false;
  let boundaryManifest: string | undefined;
  // The nearest ancestor manifest is a bounded metadata boundary, not permission
  // to link arbitrary source files outside the selected graph root.
  const initialize = (): void => {
    if (initialized) return;
    initialized = true;
    realRoot = realpathSync(root);
    let directory = realRoot;
    for (let depth = 0; depth < 32; depth++) {
      const candidate = resolve(directory, 'package.json');
      try {
        if (lstatSync(candidate).isFile()) {
          boundaryManifest = candidate;
          break;
        }
      } catch {
        /* No manifest at this ancestor. */
      }
      const parent = dirname(directory);
      if (parent === directory) break;
      directory = parent;
    }
  };
  let truncated = false;
  return {
    targets,
    truncated: () => truncated,
    resolve(specifier, importer) {
      if (!specifier.startsWith('.') || basename(specifier) !== 'package.json')
        return undefined;
      initialize();
      const absolute = resolve(realRoot, dirname(importer), specifier);
      const cached = cache.get(absolute);
      if (cached) return cached;
      let result: ImportResolution = {
        target: null,
        status: 'unresolvedInternal',
      };
      const rel = relative(realRoot, absolute);
      const inside =
        rel !== '..' && !rel.startsWith(`..${sep}`) && !isAbsolute(rel);
      if (
        (!inside && absolute !== boundaryManifest) ||
        (inside && rel.split(sep).some(part => excludeDirs.includes(part)))
      ) {
        result = { target: null, status: 'unsupported' };
      } else {
        try {
          const stat = lstatSync(absolute);
          if (
            !stat.isFile() ||
            stat.size > MAX_MANIFEST_BYTES ||
            realpathSync(absolute) !== absolute
          ) {
            result = { target: null, status: 'unsupported' };
          } else if (knownFiles.size >= maxFiles && !knownFiles.has(rel)) {
            truncated = true;
          } else {
            const value: unknown = JSON.parse(readFileSync(absolute, 'utf8'));
            if (
              value === null ||
              typeof value !== 'object' ||
              Array.isArray(value)
            ) {
              result = { target: null, status: 'unsupported' };
            } else {
              const target = rel.split(sep).join('/');
              targets.add(target);
              knownFiles.add(target);
              result = { target, status: 'resolved' };
            }
          }
        } catch {
          /* Missing or malformed metadata remains an explicit gap. */
        }
      }
      cache.set(absolute, result);
      return result;
    },
  };
}
