import { homedir } from 'node:os';
import { join, resolve } from 'node:path';
import { acquireConfigLock, fail, flag, HookHost, HookSettings, HooksInstallOptions, HooksInstallResult, hooksInstallUsage, HOSTS, loadSettings, opt, requestedHost, targetConfig, writeSettingsAtomic } from './hooks-install-specs.js';
import { awarenessHookName, entry, hasCommand, hasDriftedCommand, hasExactCommand, hookStatusKey, matchingCommandCount, obsoleteSpecsFor, removeCommand, runtimeHealth, specsFor } from './hooks-install-health.js';

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
    hookDir: options.hookDir,
  });
  const obsoleteSpecs = obsoleteSpecsFor(host, {
    globalMode,
    projectDir,
    hookDir: options.hookDir,
  });

  const checks = specs.map((spec) => {
    const groups = settings.hooks?.[spec.event];
    const present = hasCommand(groups, spec.command);
    const exact = hasExactCommand(groups, host, spec);
    const matchingCount = matchingCommandCount(groups, spec.command);
    const drifted = present && (!exact || hasDriftedCommand(groups, host, spec) || matchingCount > 1);
    return {
      key: hookStatusKey(spec),
      event: spec.event,
      hook: awarenessHookName(spec.command) ?? spec.command.split(/[\\/]/).pop(),
      installed: exact,
      present,
      matching_count: matchingCount,
      drifted,
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
    const configReady = status.installed_all && status.drifted.length === 0;
    return {
      exitCode: strict && !configReady ? 2 : 0,
      payload: {
        ok: configReady,
        action: 'check',
        strict,
        strict_scope: 'config_only',
        installed: status,
        health: {
          config: {
            status: configReady ? 'ready' : 'needs_repair',
            verified: configReady,
            settings_path: settingsPath,
          },
          runtime: runtimeHealth(host, globalMode),
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

  for (const spec of obsoleteSpecs) {
    const result = removeCommand(settings.hooks[spec.event], spec.command);
    if (!result.removed) continue;
    changed = true;
    if (result.groups.length > 0) settings.hooks[spec.event] = result.groups;
    else delete settings.hooks[spec.event];
  }

  if (flag(argv, '--remove')) {
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
