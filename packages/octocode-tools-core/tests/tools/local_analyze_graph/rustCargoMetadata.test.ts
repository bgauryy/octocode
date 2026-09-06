import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  chmod,
  mkdir,
  mkdtemp,
  realpath,
  rm,
  symlink,
  writeFile,
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { resolveRustCargoMetadata } from '../../../src/graph/rustCargoMetadata.js';

const { execute } = vi.hoisted(() => ({ execute: vi.fn() }));
vi.mock('node:child_process', () => ({ execFile: execute }));

let scratch: string;
let root: string;
let host: string;
const files = ['app/main.rs', 'shared/lib.rs'];
function metadata() {
  return {
    version: 1,
    workspace_members: ['app-id', 'shared-id'],
    packages: [
      {
        id: 'app-id',
        name: 'app',
        manifest_path: join(root, 'app/Cargo.toml'),
        targets: [
          { name: 'app', kind: ['bin'], src_path: join(root, 'app/main.rs') },
        ],
        dependencies: [
          {
            name: 'shared-package',
            rename: 'renamed_lib' as string | null,
            path: join(root, 'shared'),
            optional: false,
            kind: null,
            target: null,
          },
        ],
      },
      {
        id: 'shared-id',
        name: 'shared-package',
        manifest_path: join(root, 'shared/Cargo.toml'),
        targets: [
          {
            name: 'shared_crate',
            kind: ['lib'],
            src_path: join(root, 'shared/lib.rs'),
          },
        ],
        dependencies: [],
      },
    ],
  };
}
function respond(value: unknown) {
  execute.mockImplementation((_file, _args, _options, callback) => {
    callback(null, JSON.stringify(value), '');
    return { kill: vi.fn() };
  });
}

beforeEach(async () => {
  scratch = await mkdtemp(join(tmpdir(), 'octocode-cargo-metadata-'));
  root = join(scratch, 'repo');
  host = join(scratch, 'host');
  await mkdir(join(root, 'app'), { recursive: true });
  await mkdir(join(root, 'shared'), { recursive: true });
  await mkdir(host);
  root = await realpath(root);
  host = await realpath(host);
  await writeFile(join(root, 'Cargo.toml'), '[workspace]\n');
  await writeFile(join(host, 'cargo'), 'trusted host fixture');
  await chmod(join(host, 'cargo'), 0o755);
  for (const file of files) await writeFile(join(root, file), '');
  vi.stubEnv('PATH', host);
  execute.mockReset();
  respond(metadata());
});
afterEach(async () => {
  vi.unstubAllEnvs();
  await rm(scratch, { recursive: true, force: true });
});

describe('bounded Cargo metadata adapter', () => {
  it('preserves per-target editions for module resolution', async () => {
    const data = metadata();
    respond({
      ...data,
      packages: data.packages.map((pkg, index) => ({
        ...pkg,
        targets: pkg.targets.map(target => ({
          ...target,
          edition: index === 0 ? '2021' : '2015',
        })),
      })),
    });
    const result = await resolveRustCargoMetadata({ root, files });
    expect(result.targets).toEqual([
      expect.objectContaining({ edition: '2021' }),
      expect.objectContaining({ edition: '2015' }),
    ]);
  });

  it('uses the local library target name when the dependency is not renamed', async () => {
    const data = metadata();
    data.packages[0]!.dependencies[0]!.rename = null;
    respond(data);
    const result = await resolveRustCargoMetadata({ root, files });
    const app = result.targets.find(target => target.packageId === 'app-id')!;
    expect(app.dependencyAliases[0]!.alias).toBe('shared_crate');
    expect(app.dependencyAliases[0]!.targetId).toBe(
      result.targets.find(target => target.packageId === 'shared-id')!.id
    );
  });
  it('uses fixed offline argv and maps a renamed workspace dependency by exact path', async () => {
    const result = await resolveRustCargoMetadata({ root, files });
    expect(result.status).toBe('ok');
    expect(result.targets).toHaveLength(2);
    const app = result.targets.find(target => target.packageId === 'app-id')!;
    const library = result.targets.find(
      target => target.packageId === 'shared-id'
    )!;
    expect(app.srcPath).toBe('app/main.rs');
    expect(app.dependencyAliases).toContainEqual({
      alias: 'renamed_lib',
      packageName: 'shared-package',
      targetId: library.id,
      external: false,
      conditional: false,
    });
    expect(execute).toHaveBeenCalledWith(
      join(host, 'cargo'),
      [
        'metadata',
        '--no-deps',
        '--offline',
        '--locked',
        '--format-version=1',
        '--manifest-path',
        join(root, 'Cargo.toml'),
      ],
      expect.objectContaining({
        shell: false,
        timeout: expect.any(Number),
        maxBuffer: 1_048_576,
        env: expect.objectContaining({ RUSTUP_AUTO_INSTALL: '0' }),
      }),
      expect.any(Function)
    );
    expect(execute.mock.calls[0][2].timeout).toBeLessThanOrEqual(5000);
  });

  it('marks optional and target-conditioned dependencies unresolved', async () => {
    const data = metadata();
    data.packages[0]!.dependencies[0]!.optional = true;
    respond(data);
    const result = await resolveRustCargoMetadata({ root, files });
    const dependency = result.targets[0]!.dependencyAliases[0]!;
    expect(dependency).toMatchObject({ conditional: true, external: false });
    expect(dependency.targetId).toBeUndefined();
    expect(
      result.diagnostics.some(
        diagnostic => diagnostic.code === 'cargo-conditional-dependency'
      )
    ).toBe(true);
  });

  it('retains in-scan targets but never links an excluded workspace library as external', async () => {
    const result = await resolveRustCargoMetadata({ root, files: [files[0]!] });
    expect(result.targets).toHaveLength(1);
    expect(result.targets[0]!.dependencyAliases[0]).toMatchObject({
      external: false,
    });
    expect(result.targets[0]!.dependencyAliases[0]!.targetId).toBeUndefined();
    expect(
      result.diagnostics.some(
        diagnostic => diagnostic.code === 'cargo-target-outside-scan'
      )
    ).toBe(true);
  });

  it('rejects symlinked target escapes while preserving other targets', async () => {
    await rm(join(root, files[1]!));
    await writeFile(join(scratch, 'outside.rs'), '');
    await symlink(join(scratch, 'outside.rs'), join(root, files[1]!));
    const result = await resolveRustCargoMetadata({ root, files });
    expect(result.targets.map(target => target.srcPath)).toEqual([
      'app/main.rs',
    ]);
    expect(
      result.diagnostics.some(
        diagnostic => diagnostic.code === 'cargo-target-outside-scan'
      )
    ).toBe(true);
  });

  it.each([
    [{ code: 'ENOENT' }, 'cargo-unavailable'],
    [{ killed: true, signal: 'SIGTERM' }, 'cargo-metadata-limit'],
    [{ code: 'ERR_CHILD_PROCESS_STDIO_MAXBUFFER' }, 'cargo-metadata-limit'],
    [{ code: 101 }, 'cargo-metadata-failed'],
  ])(
    'returns unsupported coverage on process error %j',
    async (error, code) => {
      execute.mockImplementation((_file, _args, _options, callback) =>
        callback(error, '', 'private stderr')
      );
      const result = await resolveRustCargoMetadata({ root, files });
      expect(result).toMatchObject({
        status: 'unsupported',
        targets: [],
        diagnostics: [expect.objectContaining({ code })],
      });
    }
  );

  it('does not invoke repository or relative PATH executables', async () => {
    vi.stubEnv('PATH', `.:${root}`);
    const result = await resolveRustCargoMetadata({ root, files });
    expect(result.status).toBe('unsupported');
    expect(result.diagnostics[0]!.code).toBe('cargo-unavailable');
    expect(execute).not.toHaveBeenCalled();
  });

  it('reports malformed metadata instead of empty success', async () => {
    respond({ version: 1, packages: [] });
    const result = await resolveRustCargoMetadata({ root, files });
    expect(result.status).toBe('unsupported');
    expect(result.diagnostics[0]!.code).toBe('cargo-metadata-invalid');
  });

  it('reports a missing manifest without invoking Cargo', async () => {
    await rm(join(root, 'Cargo.toml'));
    const result = await resolveRustCargoMetadata({ root, files });
    expect(result.diagnostics[0]!.code).toBe('cargo-manifest-missing');
    expect(execute).not.toHaveBeenCalled();
  });

  it('resolves rustup from host home and ignores repository toolchain or wrapper overrides', async () => {
    await rm(join(host, 'cargo'));
    await writeFile(join(host, 'rustup'), 'host rustup fixture');
    await chmod(join(host, 'rustup'), 0o755);
    await symlink(join(host, 'rustup'), join(host, 'cargo'));
    const actualCargo = join(host, 'toolchain-cargo');
    await writeFile(actualCargo, 'selected host cargo');
    await chmod(actualCargo, 0o755);
    vi.stubEnv('RUSTUP_TOOLCHAIN', join(root, 'untrusted-toolchain'));
    vi.stubEnv('RUSTC_WRAPPER', join(root, 'untrusted-wrapper'));
    execute.mockImplementation((_file, args, _options, callback) =>
      callback(
        null,
        args[0] === 'which' ? `${actualCargo}\n` : JSON.stringify(metadata()),
        ''
      )
    );
    const result = await resolveRustCargoMetadata({ root, files });
    expect(result.targets).toHaveLength(2);
    expect(execute.mock.calls[0][0]).toBe(join(host, 'rustup'));
    expect(execute.mock.calls[0][1]).toEqual(['which', 'cargo']);
    expect(execute.mock.calls[0][2].cwd).not.toBe(root);
    expect(execute.mock.calls[1][0]).toBe(actualCargo);
    for (const call of execute.mock.calls) {
      expect(call[2].env.RUSTUP_TOOLCHAIN).toBeUndefined();
      expect(call[2].env.RUSTC_WRAPPER).toBeUndefined();
    }
  });

  it('rejects an out-of-root manifest symlink before invoking Cargo', async () => {
    await rm(join(root, 'Cargo.toml'));
    await writeFile(join(scratch, 'outside.toml'), '[package]');
    await symlink(join(scratch, 'outside.toml'), join(root, 'Cargo.toml'));
    const result = await resolveRustCargoMetadata({ root, files });
    expect(result.diagnostics[0]!.code).toBe('cargo-manifest-outside-root');
    expect(execute).not.toHaveBeenCalled();
  });

  it('discovers only nearest manifests on known source ancestors', async () => {
    await rm(join(root, 'Cargo.toml'));
    await writeFile(join(root, 'app/Cargo.toml'), '[package]');
    const result = await resolveRustCargoMetadata({ root, files: [files[0]!] });
    expect(result.targets).toHaveLength(1);
    expect(execute.mock.calls[0][1].at(-1)).toBe(join(root, 'app/Cargo.toml'));
  });

  it('reads a discovered workspace once and does not repeat its diagnostics', async () => {
    await rm(join(root, 'Cargo.toml'));
    await writeFile(join(root, 'app/Cargo.toml'), '[package]');
    await writeFile(join(root, 'shared/Cargo.toml'), '[package]');
    const data = metadata();
    data.packages[0]!.dependencies[0]!.optional = true;
    respond(data);
    const result = await resolveRustCargoMetadata({ root, files });
    expect(execute).toHaveBeenCalledTimes(1);
    expect(result.targets).toHaveLength(2);
    expect(result.diagnostics.map(diagnostic => diagnostic.code)).toEqual([
      'cargo-conditional-dependency',
    ]);
  });

  it('still inspects separate discovered workspaces', async () => {
    await rm(join(root, 'Cargo.toml'));
    await writeFile(join(root, 'app/Cargo.toml'), '[package]');
    await writeFile(join(root, 'shared/Cargo.toml'), '[package]');
    execute.mockImplementation((_file, args, _options, callback) => {
      const pkg = metadata().packages.find(
        candidate => candidate.manifest_path === args.at(-1)
      )!;
      callback(
        null,
        JSON.stringify({
          version: 1,
          workspace_members: [pkg.id],
          packages: [pkg],
        }),
        ''
      );
    });
    const result = await resolveRustCargoMetadata({ root, files });
    expect(execute).toHaveBeenCalledTimes(2);
    expect(result.targets.map(target => target.srcPath)).toEqual(files);
  });

  it('bounds manifest candidate probes and reports the limit explicitly', async () => {
    await rm(join(root, 'Cargo.toml'));
    const result = await resolveRustCargoMetadata({
      root,
      files: Array.from(
        { length: 80 },
        (_, index) => `directory_${index}/file.rs`
      ),
    });
    expect(result).toMatchObject({
      status: 'unsupported',
      diagnostics: [expect.objectContaining({ code: 'cargo-metadata-limit' })],
    });
    expect(execute).not.toHaveBeenCalled();
  });

  it('marks a missing same-package library as unresolved, never external', async () => {
    const data = metadata();
    data.packages[0]!.targets.push({
      name: 'app_lib',
      kind: ['lib'],
      src_path: join(root, 'app/excluded.rs'),
    });
    respond(data);
    const result = await resolveRustCargoMetadata({ root, files });
    const ownLibrary = result.targets[0]!.dependencyAliases.find(
      alias => alias.alias === 'app_lib'
    );
    expect(ownLibrary).toEqual({
      alias: 'app_lib',
      packageName: 'app',
      external: false,
      conditional: false,
    });
    expect(
      result.diagnostics.some(
        diagnostic => diagnostic.code === 'cargo-target-outside-scan'
      )
    ).toBe(true);
  });
});
