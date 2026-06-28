import { rmSync } from 'node:fs';

import type { CLICommand, ParsedArgs } from '../types.js';
import { getBool, getString } from '../options.js';
import { c, bold, dim } from '../../utils/colors.js';
import { EXIT } from '../exit-codes.js';
import {
  getLspStatus,
  TOOLCHAIN_SERVERS,
} from '@octocodeai/octocode-engine/lsp/manager';
import {
  BUNDLED_SERVER_NAMES,
  isCommandOnPath,
} from '@octocodeai/octocode-engine/lsp/config';
import {
  listManifestServers,
  manifestServer,
  manifestInstallHint,
  managedCacheRoot,
  provisionMode,
  resolveCachedServer,
} from '@octocodeai/octocode-engine/lsp/serverManifest';
import {
  provisionServer,
  uninstallServer,
} from '@octocodeai/octocode-engine/lsp/serverProvisioner';
import {
  discoverServer,
  discoverServerBatch,
} from '@octocodeai/octocode-engine/lsp/serverDiscovery';

// Toolchain-coupled servers (need a host runtime octocode won't auto-install).
// Derived from the engine's single source of truth — no local duplication.
const TOOLCHAIN: Record<string, string> = Object.fromEntries(
  TOOLCHAIN_SERVERS.map(t => [t.server, t.hint])
);

const BUNDLED_NOTE = `Bundled servers (${BUNDLED_SERVER_NAMES.join(', ')}) ship with octocode — nothing to install.`;

type Json = Record<string, unknown>;

function print(line = ''): void {
  console.log(line);
}

function emitJson(value: Json): void {
  print(JSON.stringify(value, null, 2));
}

async function runList(json: boolean): Promise<number> {
  const manifestServers = listManifestServers();
  const cwd = process.cwd();

  // One batch scan for all servers — shared cache hit for subsequent individual calls.
  const allCommands = [
    ...manifestServers.map(s => s.name),
    ...TOOLCHAIN_SERVERS.map(t => t.server),
  ];
  const discovered = discoverServerBatch(allCommands, cwd);

  const autoDownload = manifestServers.map(s => {
    const cached = resolveCachedServer(s.name);
    const ecosystem = discovered[s.name]?.command;
    const onPath = !ecosystem && isCommandOnPath(s.name) ? s.name : undefined;
    const resolved = cached ?? ecosystem ?? onPath;
    return {
      name: s.name,
      languageId: s.languageId,
      releaseTag: s.releaseTag,
      class: 'auto-download' as const,
      status: cached
        ? `installed (managed cache)`
        : resolved
          ? `available (${resolved})`
          : 'not installed',
    };
  });

  const toolchainStatus = TOOLCHAIN_SERVERS.map(t => {
    const ecosystem = discovered[t.server]?.command;
    const onPath =
      !ecosystem && isCommandOnPath(t.server) ? t.server : undefined;
    const found = ecosystem ?? onPath ?? null;
    return {
      name: t.server,
      languageId: t.languageId,
      class: 'toolchain' as const,
      status: found ? `available (${found})` : 'not installed',
      hint: t.hint,
    };
  });

  if (json) {
    emitJson({
      autoDownload,
      toolchain: toolchainStatus,
      bundledNote: BUNDLED_NOTE,
    });
    return EXIT.OK;
  }

  print(bold('Auto-downloadable language servers'));
  for (const s of autoDownload) {
    const tag =
      s.status.startsWith('installed') || s.status.startsWith('available')
        ? c('green', s.status)
        : dim(s.status);
    print(`  ${s.languageId.padEnd(12)} ${s.name.padEnd(22)} ${tag}`);
  }
  print();
  print(bold('Toolchain-required language servers'));
  for (const t of toolchainStatus) {
    const tag = t.status.startsWith('available')
      ? c('green', t.status)
      : dim(t.status);
    print(`  ${t.languageId.padEnd(12)} ${t.name.padEnd(22)} ${tag}`);
    if (!t.status.startsWith('available')) {
      print(`  ${' '.padEnd(12)} ${dim(t.hint)}`);
    }
  }
  print();
  print(dim(BUNDLED_NOTE));
  print();
  print(
    dim(
      `Auto-install policy: ${provisionMode()}  (set OCTOCODE_LSP_AUTO_INSTALL=off|auto to change, or run \`lsp-server install <name>\`)`
    )
  );
  return EXIT.OK;
}

async function runStatus(
  filePath: string | undefined,
  json: boolean
): Promise<number> {
  const status = await getLspStatus(filePath ? { filePath } : {});
  if (json) {
    emitJson(status as unknown as Json);
    return EXIT.OK;
  }
  if (!filePath) {
    print(bold('LSP status'));
    print(`  pooled clients: ${status.pooledClientCount}`);
    print(dim('  Pass a file path to see how its language server resolves.'));
    return EXIT.OK;
  }
  // `status` resolves a FILE (its language → server). A bare name like "rust"
  // is a common mix-up with `lsp-server list`. Detect that by SHAPE + the fact
  // that nothing resolved — never by stat'ing the literal path, which
  // false-negatives for paths valid against a project root but not cwd.
  const looksLikePath =
    filePath.includes('/') ||
    filePath.includes('\\') ||
    /\.[^.\\/]+$/.test(filePath);
  const unresolved =
    !status.serverAvailable &&
    (!status.languageId || status.languageId === 'plaintext');
  if (unresolved && !looksLikePath) {
    print(
      dim(
        `  '${filePath}' looks like a server name, not a file. status resolves a FILE's language → server (e.g. src/main.rs); run \`lsp-server list\` to see supported servers.`
      )
    );
  }
  print(bold(`LSP status for ${filePath}`));
  print(`  language:  ${status.languageId ?? 'unknown'}`);
  print(
    `  resolved:  ${status.serverAvailable ? c('green', String(status.serverSource)) : c('red', 'unavailable')}`
  );
  for (const hint of status.hints) print(dim(`  ${hint}`));
  return status.serverAvailable ? EXIT.OK : EXIT.OK;
}

async function runInstall(
  names: string[],
  opts: { all: boolean; force: boolean; yes: boolean; json: boolean }
): Promise<number> {
  const targets = opts.all ? listManifestServers().map(s => s.name) : names;
  if (targets.length === 0) {
    print(c('red', 'Specify a server/language to install, or use --all.'));
    return EXIT.USAGE;
  }

  const mode = opts.yes || opts.force ? 'auto' : provisionMode();
  const results: Json[] = [];
  let worstExit: number = EXIT.OK;

  for (const name of targets) {
    // Toolchain-coupled or bundled → instruct, don't fake an install.
    if (TOOLCHAIN[name]) {
      results.push({ name, action: 'instruct', message: TOOLCHAIN[name] });
      if (!opts.json) print(`${c('yellow', name)}: ${TOOLCHAIN[name]}`);
      worstExit = EXIT.NOT_FOUND;
      continue;
    }
    if (!manifestServer(name)) {
      const hint = manifestInstallHint(name);
      results.push({ name, action: 'unknown', message: hint ?? BUNDLED_NOTE });
      if (!opts.json) print(`${c('yellow', name)}: ${hint ?? BUNDLED_NOTE}`);
      worstExit = EXIT.NOT_FOUND;
      continue;
    }

    // Skip-if-exists: already in cache, or already installed elsewhere on the machine.
    if (!opts.force) {
      const cached = resolveCachedServer(name);
      const found = cached ?? discoverServer(name, process.cwd())?.command;
      if (found) {
        results.push({ name, action: 'skipped', path: found });
        if (!opts.json)
          print(`${c('green', name)}: already available (${found}) — skipping`);
        continue;
      }
    }

    const result = await provisionServer(name, { mode });
    if (result.ok) {
      results.push({ name, action: result.source, path: result.path });
      if (!opts.json)
        print(`${c('green', name)}: ${result.source} → ${result.path}`);
    } else {
      results.push({ name, action: 'error', error: result.error });
      if (!opts.json) print(`${c('red', name)}: ${result.error}`);
      worstExit = EXIT.NOT_FOUND;
    }
  }

  if (opts.json) emitJson({ install: results });
  return worstExit;
}

function runUninstall(names: string[], json: boolean): number {
  if (names.length === 0) {
    print(c('red', 'Specify a server to uninstall.'));
    return EXIT.USAGE;
  }
  const results = names.map(name => {
    const removed = uninstallServer(name);
    if (!json) {
      print(
        removed
          ? `${c('green', name)}: removed from managed cache`
          : dim(`${name}: not in managed cache (nothing to remove)`)
      );
    }
    return { name, removed };
  });
  if (json) emitJson({ uninstall: results });
  return EXIT.OK;
}

function runClean(yes: boolean, json: boolean): number {
  const root = managedCacheRoot();
  if (!yes) {
    const msg = `Would remove the managed LSP cache at ${root}. Re-run with --yes to confirm.`;
    if (json) emitJson({ clean: 'dry-run', root });
    else print(dim(msg));
    return EXIT.OK;
  }
  rmSync(root, { recursive: true, force: true });
  if (json) emitJson({ clean: 'done', root });
  else print(`${c('green', 'cleaned')}: ${root}`);
  return EXIT.OK;
}

export const lspServerCommand: CLICommand = {
  name: 'lsp-server',
  options: [
    { name: 'all' },
    { name: 'force' },
    { name: 'yes' },
    { name: 'platform', hasValue: true },
    { name: 'json' },
  ],
  handler: async (args: ParsedArgs) => {
    const sub = args.args[0];
    const rest = args.args.slice(1);
    const json = getBool(args.options, 'json');
    const platform = getString(args.options, 'platform');
    if (platform) {
      print(
        dim(
          `--platform is reserved for cross-platform pre-warm; the installer currently provisions the host platform only.`
        )
      );
    }

    let code: number;
    switch (sub) {
      case 'list':
        code = await runList(json);
        break;
      case 'status':
      case 'which':
        code = await runStatus(rest[0], json);
        break;
      case 'install':
        code = await runInstall(rest, {
          all: getBool(args.options, 'all'),
          force: getBool(args.options, 'force'),
          yes: getBool(args.options, 'yes'),
          json,
        });
        break;
      case 'uninstall':
      case 'remove':
        code = runUninstall(rest, json);
        break;
      case 'clean':
        code = runClean(getBool(args.options, 'yes'), json);
        break;
      default:
        print(c('red', `Unknown subcommand: ${sub ?? '(none)'}`));
        print(
          dim(
            'Usage: lsp-server <list|status|install|uninstall|clean> [name...]'
          )
        );
        code = EXIT.USAGE;
        break;
    }
    if (code !== EXIT.OK) process.exitCode = code;
  },
};
