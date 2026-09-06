import { realpath } from 'node:fs/promises';
import { dirname, isAbsolute, relative, resolve, sep } from 'node:path';
import { z } from 'zod';
import {
  command,
  hostCargo,
  manifests,
  MAX_BUFFER,
  MetadataFailure,
  remaining,
  TIMEOUT_MS,
  within,
} from './rustCargoProcess.js';
const LIBRARY_KINDS = new Set([
  'lib',
  'rlib',
  'dylib',
  'cdylib',
  'staticlib',
  'proc-macro',
]);

export interface RustCargoDependencyAlias {
  alias: string;
  packageName: string;
  targetId?: string;
  external: boolean;
  conditional: boolean;
}
export interface RustCargoTarget {
  id: string;
  packageId: string;
  packageName: string;
  crateName: string;
  kind: string[];
  edition?: string;
  srcPath: string;
  dependencyAliases: RustCargoDependencyAlias[];
}
export interface RustCargoDiagnostic {
  code: string;
  message: string;
}
export interface RustCargoMetadataResult {
  status: 'ok' | 'unsupported';
  targets: RustCargoTarget[];
  diagnostics: RustCargoDiagnostic[];
}

const dependencySchema = z.object({
  name: z.string(),
  rename: z.string().nullable().optional(),
  path: z.string().optional(),
  optional: z.boolean().default(false),
  kind: z.string().nullable().optional(),
  target: z.string().nullable().optional(),
});
const packageSchema = z.object({
  id: z.string(),
  name: z.string(),
  manifest_path: z.string(),
  targets: z
    .array(
      z.object({
        name: z.string(),
        kind: z.array(z.string()).min(1),
        src_path: z.string(),
        edition: z.string().optional(),
      })
    )
    .min(1),
  dependencies: z.array(dependencySchema),
});
const metadataSchema = z.object({
  version: z.literal(1),
  workspace_members: z.array(z.string()).min(1),
  packages: z.array(packageSchema).min(1),
});
type CargoPackage = z.infer<typeof packageSchema>;

async function collectTargets(
  root: string,
  known: ReadonlySet<string>,
  packages: CargoPackage[],
  diagnostics: RustCargoDiagnostic[],
  deadline: number
): Promise<RustCargoTarget[]> {
  const targets: RustCargoTarget[] = [];
  const byDirectory = new Map<string, CargoPackage[]>();
  for (const pkg of packages) {
    remaining(deadline);
    if (!isAbsolute(pkg.manifest_path))
      throw new MetadataFailure(
        'cargo-metadata-invalid',
        'Cargo package manifests must use absolute paths.'
      );
    const directory = await realpath(dirname(pkg.manifest_path)).catch(() =>
      resolve(dirname(pkg.manifest_path))
    );
    byDirectory.set(directory, [...(byDirectory.get(directory) ?? []), pkg]);
    for (const target of pkg.targets) {
      remaining(deadline);
      const source = isAbsolute(target.src_path)
        ? resolve(target.src_path)
        : '';
      const canonical = source ? await realpath(source).catch(() => '') : '';
      const srcPath = canonical
        ? relative(root, canonical).split(sep).join('/')
        : '';
      if (
        !source ||
        !canonical ||
        !within(root, canonical) ||
        !known.has(srcPath)
      ) {
        diagnostics.push({
          code: 'cargo-target-outside-scan',
          message: `Cargo target ${pkg.name}/${target.name} is outside the scanned file set; its crate context is unsupported.`,
        });
        continue;
      }
      targets.push({
        id: JSON.stringify([pkg.id, target.name, target.kind, source]),
        packageId: pkg.id,
        packageName: pkg.name,
        crateName: target.name.replaceAll('-', '_'),
        kind: target.kind,
        ...(target.edition !== undefined ? { edition: target.edition } : {}),
        srcPath,
        dependencyAliases: [],
      });
    }
  }
  for (const target of targets) {
    const pkg = packages.find(candidate => candidate.id === target.packageId)!;
    const isBuild = target.kind.includes('custom-build');
    for (const dependency of pkg.dependencies) {
      remaining(deadline);
      if (isBuild !== (dependency.kind === 'build')) continue;
      const conditional =
        dependency.optional ||
        Boolean(dependency.target) ||
        dependency.kind === 'dev';
      const directory =
        dependency.path && isAbsolute(dependency.path)
          ? await realpath(dependency.path).catch(() =>
              resolve(dependency.path!)
            )
          : undefined;
      const members = directory ? (byDirectory.get(directory) ?? []) : [];
      const libraries =
        members.length === 1
          ? members[0]!.targets.filter(candidate =>
              candidate.kind.some(kind => LIBRARY_KINDS.has(kind))
            )
          : [];
      // Cargo's extern_crate_name_and_dep_name uses an explicit rename first,
      // otherwise the library target's crate name, which can differ from the package.
      const alias = (
        dependency.rename ??
        (libraries.length === 1 ? libraries[0]!.name : dependency.name)
      ).replaceAll('-', '_');
      const localTargets =
        members.length === 1
          ? targets.filter(
              candidate =>
                candidate.packageId === members[0]!.id &&
                candidate.kind.some(kind => LIBRARY_KINDS.has(kind))
            )
          : [];
      const linked =
        !conditional && localTargets.length === 1 ? localTargets[0] : undefined;
      target.dependencyAliases.push({
        alias,
        packageName: dependency.name,
        ...(linked ? { targetId: linked.id } : {}),
        external: members.length === 0,
        conditional,
      });
      if (conditional)
        diagnostics.push({
          code: 'cargo-conditional-dependency',
          message: `Dependency ${alias} of ${pkg.name} is conditional; metadata without dependency resolution does not establish an active import.`,
        });
      else if (members.length && !linked)
        diagnostics.push({
          code: 'cargo-workspace-dependency-unresolved',
          message: `Workspace dependency ${alias} of ${pkg.name} has no unique library target in this scan.`,
        });
    }
    if (!isBuild && !target.kind.some(kind => LIBRARY_KINDS.has(kind))) {
      const libraries = targets.filter(
        candidate =>
          candidate.packageId === pkg.id &&
          candidate.kind.some(kind => LIBRARY_KINDS.has(kind))
      );
      for (const library of pkg.targets.filter(candidate =>
        candidate.kind.some(kind => LIBRARY_KINDS.has(kind))
      )) {
        const linked = libraries.length === 1 ? libraries[0] : undefined;
        target.dependencyAliases.push({
          alias: library.name.replaceAll('-', '_'),
          packageName: pkg.name,
          ...(linked ? { targetId: linked.id } : {}),
          external: false,
          conditional: false,
        });
      }
    }
  }
  return targets;
}

/** Called only by the explicit rustWorkspace:'cargo' graph mode. Never builds code. */
export async function resolveRustCargoMetadata(input: {
  root: string;
  files: readonly string[];
}): Promise<RustCargoMetadataResult> {
  const deadline = Date.now() + TIMEOUT_MS;
  const diagnostics: RustCargoDiagnostic[] = [];
  const targets: RustCargoTarget[] = [];
  try {
    const root = await realpath(resolve(input.root));
    const known = new Set(
      input.files.map(file =>
        relative(root, resolve(root, file)).split(sep).join('/')
      )
    );
    const discovered = await manifests(root, input.files, deadline);
    const cargo = await hostCargo(root, deadline);
    const visitedPackages = new Set<string>();
    const visitedManifests = new Set<string>();
    let outputBytes = 0;
    for (const manifest of discovered) {
      remaining(deadline);
      if (visitedManifests.has(manifest)) continue;
      const stdout = await command(
        cargo.executable,
        [
          'metadata',
          '--no-deps',
          '--offline',
          '--locked',
          '--format-version=1',
          '--manifest-path',
          manifest,
        ],
        dirname(manifest),
        cargo.env,
        deadline
      );
      outputBytes += Buffer.byteLength(stdout, 'utf8');
      if (outputBytes > MAX_BUFFER)
        throw new MetadataFailure(
          'cargo-metadata-limit',
          'Combined Cargo metadata output exceeded one MiB.'
        );
      let value: unknown;
      try {
        value = JSON.parse(stdout);
      } catch {
        throw new MetadataFailure(
          'cargo-metadata-invalid',
          'Cargo returned malformed metadata JSON.'
        );
      }
      const parsed = metadataSchema.safeParse(value);
      if (!parsed.success)
        throw new MetadataFailure(
          'cargo-metadata-invalid',
          'Cargo metadata does not satisfy format version 1 workspace requirements.'
        );
      const members = new Set(parsed.data.workspace_members);
      const packages = parsed.data.packages.filter(pkg => members.has(pkg.id));
      const collected = await collectTargets(
        root,
        known,
        packages,
        diagnostics,
        deadline
      );
      targets.push(
        ...collected.filter(target => !visitedPackages.has(target.packageId))
      );
      for (const pkg of packages) {
        visitedPackages.add(pkg.id);
        // A member invocation already describes its entire workspace. Match
        // canonical manifest identities before launching another subprocess.
        visitedManifests.add(
          await realpath(pkg.manifest_path).catch(() =>
            resolve(pkg.manifest_path)
          )
        );
      }
      remaining(deadline);
    }
    if (!targets.length && !diagnostics.length)
      diagnostics.push({
        code: 'cargo-target-outside-scan',
        message:
          'Cargo metadata contained no workspace targets within this scan.',
      });
  } catch (error) {
    diagnostics.push(
      error instanceof MetadataFailure
        ? { code: error.code, message: error.message }
        : {
            code: 'cargo-metadata-failed',
            message:
              'Cargo workspace metadata could not be inspected within the scanned root.',
          }
    );
  }
  return {
    status: targets.length ? 'ok' : 'unsupported',
    targets,
    diagnostics,
  };
}
