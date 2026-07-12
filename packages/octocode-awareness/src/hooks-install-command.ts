import { homedir } from 'node:os';
import { join, resolve } from 'node:path';
import { connectDb } from './db.js';
import { hookReceipts, hookRuntimeReceiptHealth } from './hook-receipts.js';
import { acquireConfigLock, fail, flag, HookHost, HookSettings, HooksInstallOptions, HooksInstallResult, hooksInstallUsage, HOSTS, loadSettings, opt, projectHookDir, requestedHost, targetConfig, writeSettingsAtomic } from './hooks-install-specs.js';
import { awarenessHookName, entry, frontmatterHookDefinition, hasCommand, hasDriftedCommand, hasExactCommand, hookStatusKey, hookTargetExists, matchingCommandCount, obsoleteSpecsFor, removeCommand, removeUnexpectedAwarenessCommands, runtimeHealth, specsFor } from './hooks-install-health.js';

export function runHooksInstall(argv: string[], options: HooksInstallOptions): HooksInstallResult {
  const hostValue = requestedHost(argv);
  const writes = !flag(argv, '--help')
    && !flag(argv, '-h')
    && !flag(argv, '--check')
    && !flag(argv, '--dry-run')
    && !(flag(argv, '--global') && argv.includes('--project-dir'))
    && HOSTS.has(hostValue as HookHost);
  if (!writes) return runHooksInstallUnlocked(argv, options);

  const host = hostValue as HookHost;
  const cwd = options.cwd ?? process.cwd();
  const home = options.homeDir ?? homedir();
  const config = targetConfig(host);
  const settingsPath = flag(argv, '--global')
    ? join(home, config.dir, config.file)
    : join(resolve(opt(argv, '--project-dir', cwd)), config.dir, config.file);

  let release: (() => void) | undefined;
  try {
    release = acquireConfigLock(settingsPath);
    return runHooksInstallUnlocked(argv, options);
  } catch (error) {
    return fail(`cannot update ${settingsPath}: ${(error as Error).message}`);
  } finally {
    release?.();
  }
}

export function runHooksInstallUnlocked(argv: string[], options: HooksInstallOptions): HooksInstallResult {
  if (flag(argv, '--help') || flag(argv, '-h')) {
    return { exitCode: 0, text: hooksInstallUsage() + '\n' };
  }
  if (flag(argv, '--global') && argv.includes('--project-dir')) {
    return fail('use either --global or --project-dir, not both');
  }
  if (flag(argv, '--check') && !argv.includes('--host')) {
    return fail('hooks check requires --host claude, --host codex, or --host cursor');
  }

  const hostValue = requestedHost(argv);
  if (!HOSTS.has(hostValue as HookHost)) {
    return fail('invalid --host; expected claude, codex, or cursor', { host: hostValue });
  }

  const host = hostValue as HookHost;
  const cwd = options.cwd ?? process.cwd();
  const home = options.homeDir ?? homedir();
  const globalMode = flag(argv, '--global');
  const projectDir = resolve(opt(argv, '--project-dir', cwd));
  const config = targetConfig(host);
  const settingsPath = globalMode
    ? join(home, config.dir, config.file)
    : join(projectDir, config.dir, config.file);

  let settings: HookSettings;
  try {
    settings = loadSettings(settingsPath);
  } catch (error) {
    return fail(`cannot parse ${settingsPath}: ${(error as Error).message}`);
  }

  const specs = specsFor(host, {
    globalMode,
    projectDir,
    hookDir: projectHookDir(host, globalMode, projectDir, options.hookDir),
  });
  const obsoleteSpecs = obsoleteSpecsFor(host, {
    globalMode,
    projectDir,
    hookDir: projectHookDir(host, globalMode, projectDir, options.hookDir),
  });

  const checks = specs.map((spec) => {
    const groups = settings.hooks?.[spec.event];
    const present = hasCommand(groups, spec.command);
    const exact = hasExactCommand(groups, host, spec);
    const matchingCount = matchingCommandCount(groups, spec.command);
    const targetExists = hookTargetExists(spec);
    const drifted = present && (!exact || !targetExists || hasDriftedCommand(groups, host, spec) || matchingCount > 1);
    return {
      key: hookStatusKey(spec),
      event: spec.event,
      hook: awarenessHookName(spec.command) ?? spec.command.split(/[\\/]/).pop(),
      installed: exact && targetExists,
      present,
      matching_count: matchingCount,
      drifted,
      target_path: spec.targetPath,
      target_exists: targetExists,
      issue: targetExists ? null : 'target_missing',
      expected: {
        matcher: spec.matcher ?? null,
        command: spec.command,
        command_windows: spec.commandWindows ?? null,
        timeout: 20,
        shape: host === 'cursor' ? 'flat' : 'nested',
      },
    };
  });
  const hooks = Object.fromEntries(checks.map((check) => [check.key, check.installed]));
  const obsolete = obsoleteSpecs
    .filter((spec) => hasCommand(settings.hooks?.[spec.event], spec.command))
    .map(hookStatusKey);
  const status = {
    host,
    settingsPath,
    hooks,
    installed_all: checks.every((check) => check.installed) && obsolete.length === 0,
    missing: checks.filter((check) => !check.present).map((check) => check.key),
    drifted: [...checks.filter((check) => check.drifted).map((check) => check.key), ...obsolete],
    details: Object.fromEntries(checks.map((check) => [check.key, check])),
  };

  if (flag(argv, '--check')) {
    const strict = flag(argv, '--strict');
    const settingsPresent = checks.some((check) => check.present) || obsolete.length > 0;
    const definition = host === 'claude' && !globalMode
      ? frontmatterHookDefinition(projectDir, specs)
      : { exists: false, complete: false, path: null };
    const frontmatterSurface = !settingsPresent && definition.exists;
    const configReady = frontmatterSurface
      ? definition.complete
      : status.installed_all && status.drifted.length === 0;
    let receiptHealth = hookRuntimeReceiptHealth([], specs.map((spec) => spec.event));
    if (options.dbPath) {
      let database: ReturnType<typeof connectDb> | undefined;
      try {
        database = connectDb(options.dbPath);
        receiptHealth = hookRuntimeReceiptHealth(hookReceipts(database, projectDir, host), specs.map((spec) => spec.event));
      } catch {
        // A missing/unreadable receipt store is unverified, never inferred healthy.
      } finally {
        try { database?.close(); } catch { /* best effort */ }
      }
    }
    const surface = frontmatterSurface ? 'skill_frontmatter' : 'settings';
    const compactRuntime = {
      runtime: receiptHealth.status,
      coverage: receiptHealth.coverage,
      ...(receiptHealth.last_seen ? { last_seen: receiptHealth.last_seen } : {}),
    };
    if (flag(argv, '--compact')) {
      return {
        exitCode: strict && !configReady ? 2 : 0,
        payload: {
          ok: configReady,
          action: 'check',
          host,
          surface,
          ...(frontmatterSurface ? {} : { hook_count: checks.length, missing: status.missing, drifted: status.drifted }),
          health: {
            ...(frontmatterSurface ? {
              definition: definition.complete ? 'ready' : 'needs_repair',
              config: 'not_required',
              activation: 'unverified',
            } : { config: configReady ? 'ready' : 'needs_repair' }),
            ...compactRuntime,
          },
        },
      };
    }
    return {
      exitCode: strict && !configReady ? 2 : 0,
      payload: {
        ok: configReady,
        action: 'check',
        strict,
        strict_scope: frontmatterSurface ? 'definition_only' : 'config_only',
        surface,
        installed: status,
        health: {
          ...(frontmatterSurface ? {
            definition: { status: definition.complete ? 'ready' : 'needs_repair', path: definition.path },
            config: { status: 'not_required', verified: true },
            activation: { status: 'unverified' },
          } : {
            config: {
              status: configReady ? 'ready' : 'needs_repair',
              verified: configReady,
              settings_path: settingsPath,
            },
          }),
          runtime: {
            ...runtimeHealth(host, globalMode),
            status: receiptHealth.status,
            verified: receiptHealth.status === 'observed',
            last_seen: receiptHealth.last_seen,
            coverage: receiptHealth.coverage,
          },
        },
      },
    };
  }

  let changed = false;
  settings.hooks ??= {};
  if (host === 'cursor' && !flag(argv, '--remove') && settings.version == null) {
    settings.version = 1;
    changed = true;
  }

  const removing = flag(argv, '--remove');
  const expectedByEvent = new Map<string, Set<string>>();
  if (!removing) {
    for (const spec of specs) {
      const name = awarenessHookName(spec.command);
      if (!name) continue;
      const names = expectedByEvent.get(spec.event) ?? new Set<string>();
      names.add(name);
      expectedByEvent.set(spec.event, names);
    }
  }
  for (const [event, groups] of Object.entries(settings.hooks)) {
    const cleaned = removeUnexpectedAwarenessCommands(groups, expectedByEvent.get(event));
    if (!cleaned.removed) continue;
    changed = true;
    if (cleaned.groups.length > 0) settings.hooks[event] = cleaned.groups;
    else delete settings.hooks[event];
  }

  for (const spec of obsoleteSpecs) {
    const result = removeCommand(settings.hooks[spec.event], spec.command);
    if (!result.removed) continue;
    changed = true;
    if (result.groups.length > 0) settings.hooks[spec.event] = result.groups;
    else delete settings.hooks[spec.event];
  }

  if (removing) {
    for (const spec of specs) {
      const result = removeCommand(settings.hooks[spec.event], spec.command);
      if (result.removed) {
        changed = true;
        if (result.groups.length > 0) settings.hooks[spec.event] = result.groups;
        else delete settings.hooks[spec.event];
      }
    }
    if (Object.keys(settings.hooks).length === 0) delete settings.hooks;
  } else {
    const checksByKey = new Map(checks.map((check) => [check.key, check]));
    for (const spec of specs) {
      const groups = settings.hooks[spec.event] ?? [];
      settings.hooks[spec.event] = groups;
      const check = checksByKey.get(hookStatusKey(spec));
      if (!check?.installed || check.drifted) {
        const pruned = removeCommand(groups, spec.command);
        settings.hooks[spec.event] = pruned.groups;
        settings.hooks[spec.event]!.push(entry(host, spec));
        changed = true;
      }
    }
  }

  if (flag(argv, '--dry-run')) {
    if (flag(argv, '--compact')) {
      return {
        exitCode: 0,
        payload: {
          ok: true,
          action: 'dry-run',
          host,
          changed,
          settings_path: settingsPath,
          hook_count: specs.length,
        },
      };
    }
    return {
      exitCode: 0,
      payload: {
        ok: true,
        action: 'dry-run',
        host,
        changed,
        settingsPath,
        resultingSettings: settings,
        runtime: runtimeHealth(host, globalMode),
      },
    };
  }

  if (changed) {
    writeSettingsAtomic(settingsPath, settings);
  }

  if (flag(argv, '--compact')) {
    return {
      exitCode: 0,
      payload: {
        ok: true,
        action: flag(argv, '--remove') ? 'remove' : 'install',
        host,
        changed,
        settings_path: settingsPath,
        hook_count: specs.length,
      },
    };
  }

  return {
    exitCode: 0,
    payload: {
      ok: true,
      action: flag(argv, '--remove') ? 'remove' : 'install',
      host,
      changed,
      settingsPath,
      note: changed ? `${settingsPath.split(/[\\/]/).pop()} updated` : 'already up to date - no change',
      runtime: runtimeHealth(host, globalMode),
    },
  };
}
