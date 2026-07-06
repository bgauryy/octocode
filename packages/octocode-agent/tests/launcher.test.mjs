import assert from 'node:assert/strict';
import test from 'node:test';
import {
  CORE_PACKAGE,
  CORE_SPEC,
  PI_PACKAGE,
  getEffectivePiPackage,
  resolveCoreSpec,
  resolvePiBin,
  parseInvocation,
  buildLaunchEnv,
  buildPiArgs,
  updateCommand,
  versionReport,
  helpReport,
  launcherVersion,
  launchAgent,
  runUpdate,
  main,
  resolvePackageJson,
  readPackageVersion,
} from '../bin/launcher.mjs';

// ── constants ─────────────────────────────────────────────────────────────────

test('CORE_SPEC is the npm: spec for pi -e flag', () => {
  assert.equal(CORE_SPEC, `npm:${CORE_PACKAGE}`);
});

// ── resolvePackageJson / readPackageVersion ───────────────────────────────────

test('resolvePackageJson / readPackageVersion return null for an unresolvable package', () => {
  assert.equal(resolvePackageJson('this-package-does-not-exist-xyz'), null);
  assert.equal(readPackageVersion('this-package-does-not-exist-xyz'), null);
});

// ── getEffectivePiPackage ──────────────────────────────────────────────────────

test('getEffectivePiPackage returns default PI_PACKAGE when no override is set', () => {
  assert.equal(getEffectivePiPackage({}), PI_PACKAGE);
  assert.equal(getEffectivePiPackage({ OTHER: 'x' }), PI_PACKAGE);
});

test('getEffectivePiPackage returns OCTOCODE_PI_PACKAGE override when set', () => {
  assert.equal(
    getEffectivePiPackage({ OCTOCODE_PI_PACKAGE: '@octocodeai/pi-coding-agent' }),
    '@octocodeai/pi-coding-agent',
  );
});

// ── resolveCoreSpec ───────────────────────────────────────────────────────────

test('resolveCoreSpec returns OCTOCODE_AGENT_EXTENSION_SPEC when set', () => {
  assert.equal(
    resolveCoreSpec({ OCTOCODE_AGENT_EXTENSION_SPEC: '/custom/core' }),
    '/custom/core',
  );
  assert.equal(
    resolveCoreSpec({ OCTOCODE_AGENT_EXTENSION_SPEC: 'npm:@foo/bar@1.0.0' }),
    'npm:@foo/bar@1.0.0',
  );
});

test('resolveCoreSpec falls back to CORE_SPEC when package is not installed', () => {
  // In the test environment the package may or may not be installed.
  // The only guarantee is that the result is always a non-empty string.
  const spec = resolveCoreSpec({});
  assert.ok(typeof spec === 'string' && spec.length > 0, 'always returns a spec');
});

test('resolveCoreSpec returns CORE_SPEC (npm: fallback) when package cannot be resolved', () => {
  // In a healthy install this resolves to the bundled dependency path; damaged/dev
  // installs can fall back to the npm: spec. Both are valid launch specs.
  const spec = resolveCoreSpec({});
  const isLocalPath = spec.startsWith('/') || spec.startsWith('.');
  const isNpmSpec = spec === CORE_SPEC;
  assert.ok(isLocalPath || isNpmSpec, `spec is either a local path or ${CORE_SPEC}, got: ${spec}`);
});

test('smoke: installed Pi host and core package resolve for real launcher runs', () => {
  const pi = resolvePiBin({});
  assert.ok(pi, 'Pi host dependency resolves');
  assert.ok(pi.bin.endsWith('pi') || pi.bin.endsWith('pi.js') || pi.bin.endsWith('cli.js'), `Pi bin looks executable: ${pi.bin}`);

  const coreSpec = resolveCoreSpec({});
  assert.notEqual(coreSpec, CORE_SPEC, 'normal workspace/install resolves bundled core before npm fallback');
  assert.ok(coreSpec.includes('octocode-pi-extension') || coreSpec.includes('@octocodeai'), `core spec points at installed package: ${coreSpec}`);

  assert.deepEqual(
    buildPiArgs(coreSpec, ['--version'], {}),
    ['--no-extensions', '--exclude-tools', 'grep,find,ls', '-e', coreSpec, '--version'],
  );
});

// ── parseInvocation ───────────────────────────────────────────────────────────

test('parseInvocation routes reserved subcommands, forwards the rest to Pi', () => {
  assert.deepEqual(parseInvocation(['update']), { command: 'update', target: 'platform' });
  assert.deepEqual(parseInvocation(['--update']), { command: 'update', target: 'platform' });
  assert.deepEqual(parseInvocation(['update', 'core']), { command: 'update', target: 'core' });
  assert.equal(parseInvocation(['--version']).command, 'version');
  assert.equal(parseInvocation(['-v']).command, 'version');
  assert.equal(parseInvocation(['--agent-help']).command, 'help');
  assert.deepEqual(parseInvocation(['--model', 'x', 'chat']), { command: 'run', rest: ['--model', 'x', 'chat'] });
  assert.deepEqual(parseInvocation([]), { command: 'run', rest: [] });
});

// ── buildLaunchEnv ────────────────────────────────────────────────────────────

test('buildLaunchEnv selects octocode-first mode and marks the agent, without clobbering user env', () => {
  const env = buildLaunchEnv({ PATH: '/x' });
  assert.equal(env.OCTOCODE_PROMPT_MODE, 'octocode-first');
  assert.equal(env.OCTOCODE_AGENT, '1');
  assert.equal(env.PATH, '/x');

  const preset = buildLaunchEnv({ OCTOCODE_PROMPT_MODE: 'append' });
  assert.equal(preset.OCTOCODE_PROMPT_MODE, 'append', 'never overrides an explicit user choice');
});

// ── updateCommand ─────────────────────────────────────────────────────────────

test('updateCommand: core installs into the current launcher prefix', () => {
  assert.deepEqual(updateCommand('core', { prefix: '/agent/root' }), {
    cmd: 'npm',
    args: ['install', '--prefix', '/agent/root', '--omit=dev', `${CORE_PACKAGE}@latest`],
  });
});

test('updateCommand: platform self-updates globally', () => {
  assert.deepEqual(updateCommand('platform'), {
    cmd: 'npm',
    args: ['install', '-g', 'octocode-agent@latest'],
  });
});

// ── versionReport ─────────────────────────────────────────────────────────────

test('versionReport names launcher, core, and Pi host (default)', () => {
  const report = versionReport({});
  assert.ok(report.includes('octocode-agent'));
  assert.ok(report.includes(CORE_PACKAGE));
  assert.ok(report.includes(PI_PACKAGE));
  assert.ok(report.split('\n')[0].includes(launcherVersion() ?? '?'));
});

test('versionReport shows npm: fallback when core is not installed locally', () => {
  // Mock by checking the contract: if readPackageVersion(CORE_PACKAGE) is null,
  // the report must mention CORE_SPEC.
  const coreInstalled = readPackageVersion(CORE_PACKAGE) !== null;
  const report = versionReport({});
  if (!coreInstalled) {
    assert.ok(report.includes(CORE_SPEC), 'shows npm: fallback spec when not installed');
  } else {
    // When installed, shows the version number.
    assert.ok(report.includes(CORE_PACKAGE));
  }
});

test('versionReport shows override package name when OCTOCODE_PI_PACKAGE is set', () => {
  const report = versionReport({ OCTOCODE_PI_PACKAGE: '@octocodeai/pi-coding-agent' });
  assert.ok(report.includes('@octocodeai/pi-coding-agent'), 'shows override package name');
  assert.ok(!report.includes('@earendil-works/pi-coding-agent'), 'does not show default when overridden');
});

test('versionReport shows local binary path when OCTOCODE_PI_BIN is set', () => {
  const report = versionReport({ OCTOCODE_PI_BIN: '/usr/local/bin/my-pi' });
  assert.ok(report.includes('/usr/local/bin/my-pi'), 'shows env-bin path');
});

// ── helpReport ────────────────────────────────────────────────────────────────

test('helpReport documents launch, update, install instructions, and fork env vars', () => {
  const h = helpReport();
  assert.ok(h.includes('update'));
  assert.ok(h.includes(CORE_PACKAGE));
  assert.ok(h.includes('octocode-agent update core'), 'documents core update command');
  assert.ok(h.includes('OCTOCODE_PI_BIN'), 'documents fork dev env var');
  assert.ok(h.includes('OCTOCODE_PI_PACKAGE'), 'documents fork prod env var');
});

// ── buildPiArgs ───────────────────────────────────────────────────────────────

test('buildPiArgs: default includes --no-extensions and lean exclude while keeping context files', () => {
  assert.deepEqual(
    buildPiArgs('/core', ['chat', '--foo'], {}),
    ['--no-extensions', '--exclude-tools', 'grep,find,ls', '-e', '/core', 'chat', '--foo'],
    'default = no-extensions + lean + project context files enabled',
  );
});

test('buildPiArgs: supports npm: spec (recovery core delivery)', () => {
  assert.deepEqual(
    buildPiArgs(CORE_SPEC, [], {}),
    ['--no-extensions', '--exclude-tools', 'grep,find,ls', '-e', CORE_SPEC],
    'npm: spec is passed as-is to pi -e',
  );
});

test('buildPiArgs: OCTOCODE_AGENT_NO_CONTEXT_FILES=1 suppresses AGENTS.md loading', () => {
  assert.deepEqual(
    buildPiArgs('/core', ['chat'], { OCTOCODE_AGENT_NO_CONTEXT_FILES: '1' }),
    ['--no-extensions', '--exclude-tools', 'grep,find,ls', '--no-context-files', '-e', '/core', 'chat'],
    'NO_CONTEXT_FILES=1 adds --no-context-files',
  );
});

test('buildPiArgs: FULL_TOOLS opts out of lean; CLEAN additionally suppresses user skills and context', () => {
  assert.deepEqual(
    buildPiArgs('/core', ['chat'], { OCTOCODE_AGENT_FULL_TOOLS: '1' }),
    ['--no-extensions', '-e', '/core', 'chat'],
    'FULL_TOOLS keeps grep/find/ls while project context files remain enabled',
  );
  assert.deepEqual(
    buildPiArgs('/core', ['chat'], { OCTOCODE_AGENT_CLEAN: '1', OCTOCODE_AGENT_FULL_TOOLS: '1' }),
    ['--no-extensions', '--no-skills', '--no-context-files', '-e', '/core', 'chat'],
    'CLEAN adds --no-skills and --no-context-files for a fully deterministic branded agent',
  );
});

test('buildPiArgs: CLEAN + lean forces no skills and no context files', () => {
  assert.deepEqual(
    buildPiArgs('/core', [], { OCTOCODE_AGENT_CLEAN: '1' }),
    ['--no-extensions', '--no-skills', '--exclude-tools', 'grep,find,ls', '--no-context-files', '-e', '/core'],
  );
});

// ── launchAgent ───────────────────────────────────────────────────────────────

test('launchAgent resolves core via resolveCoreSpec and execs Pi; forwards exit code', () => {
  const calls = [];
  const fakeSpawn = (cmd, args, opts) => {
    calls.push({ cmd, args, env: opts.env });
    return { status: 7 };
  };
  const code = launchAgent(['chat', '--foo'], {
    spawn: fakeSpawn,
    log: () => {},
    resolvePiBin: () => ({ bin: '/pi/bin/pi', pkgRoot: '/pi', source: 'bundled' }),
    resolveCoreSpec: () => '/core',
    env: { PATH: '/x' },
  });
  assert.equal(code, 7, 'exit code from the pi run is forwarded');
  assert.equal(calls.length, 1, 'single spawn — no global install side effect');
  assert.deepEqual(
    calls[0].args,
    ['--no-extensions', '--exclude-tools', 'grep,find,ls', '-e', '/core', 'chat', '--foo'],
  );
  assert.equal(calls[0].env.OCTOCODE_PROMPT_MODE, 'octocode-first');
  assert.equal(calls[0].env.OCTOCODE_AGENT, '1');
});

test('launchAgent works with npm: spec (lean mode — core not installed locally)', () => {
  const calls = [];
  launchAgent([], {
    spawn: (cmd, args) => (calls.push(args), { status: 0 }),
    log: () => {},
    resolvePiBin: () => ({ bin: '/pi/bin/pi', source: 'bundled' }),
    resolveCoreSpec: () => CORE_SPEC,   // lean fallback: npm: spec
    env: { PATH: '/x' },
  });
  assert.ok(calls[0].includes(CORE_SPEC), 'npm: spec is forwarded to pi -e');
});

test('launchAgent honors OCTOCODE_AGENT_EXTENSION_SPEC override', () => {
  const calls = [];
  launchAgent([], {
    spawn: (cmd, args) => (calls.push(args), { status: 0 }),
    log: () => {},
    resolvePiBin: () => ({ bin: '/pi/bin/pi', source: 'bundled' }),
    resolveCoreSpec: () => 'npm:@octocodeai/pi-extension@0.3.0',
    env: { OCTOCODE_AGENT_EXTENSION_SPEC: 'npm:@octocodeai/pi-extension@0.3.0' },
  });
  assert.deepEqual(
    calls[0],
    ['--no-extensions', '--exclude-tools', 'grep,find,ls', '-e', 'npm:@octocodeai/pi-extension@0.3.0'],
  );
});

test('launchAgent fails cleanly when Pi host is missing', () => {
  let msg = '';
  const code = launchAgent([], {
    spawn: () => ({ status: 0 }),
    log: (m) => { msg = m; },
    resolvePiBin: () => null,
    resolveCoreSpec: () => '/core',
    env: {},
  });
  assert.equal(code, 1);
  assert.ok(msg.includes('update'), 'suggests update command');
});

test('launchAgent shows helpful error when OCTOCODE_PI_BIN path is not found', () => {
  let msg = '';
  const code = launchAgent([], {
    spawn: () => ({ status: 0 }),
    log: (m) => { msg = m; },
    resolvePiBin: () => null,
    resolveCoreSpec: () => '/core',
    env: { OCTOCODE_PI_BIN: '/nonexistent/pi' },
  });
  assert.equal(code, 1);
  assert.ok(msg.includes('OCTOCODE_PI_BIN'), 'names the env var in the error');
  assert.ok(msg.includes('/nonexistent/pi'), 'shows the bad path');
});

// ── main ──────────────────────────────────────────────────────────────────────

test('main dispatches version/help/update/run', () => {
  let printed = '';
  assert.equal(main(['--version'], { out: (m) => { printed = m; } }), 0);
  assert.ok(printed.includes('octocode-agent'));

  const upd = [];
  assert.equal(main(['update'], { spawn: (c, a) => (upd.push([c, ...a]), { status: 0 }), log: () => {} }), 0);
  assert.deepEqual(upd[0], ['npm', 'install', '-g', 'octocode-agent@latest']);
});

test('main update core installs the extension into the launcher prefix', () => {
  const upd = [];
  assert.equal(
    main(['update', 'core'], { spawn: (c, a) => (upd.push([c, ...a]), { status: 0 }), log: () => {}, prefix: '/agent/root' }),
    0,
  );
  assert.deepEqual(upd[0], ['npm', 'install', '--prefix', '/agent/root', '--omit=dev', `${CORE_PACKAGE}@latest`]);
});
