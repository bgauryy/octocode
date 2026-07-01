import assert from 'node:assert/strict';
import test from 'node:test';
import {
  CORE_PACKAGE,
  PI_PACKAGE,
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

test('resolvePackageJson / readPackageVersion return null for an unresolvable package', () => {
  assert.equal(resolvePackageJson('this-package-does-not-exist-xyz'), null);
  assert.equal(readPackageVersion('this-package-does-not-exist-xyz'), null);
});

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

test('buildLaunchEnv selects replace mode and marks the agent, without clobbering user env', () => {
  const env = buildLaunchEnv({ PATH: '/x' });
  assert.equal(env.OCTOCODE_PROMPT_MODE, 'replace');
  assert.equal(env.OCTOCODE_AGENT, '1');
  assert.equal(env.PATH, '/x');

  const preset = buildLaunchEnv({ OCTOCODE_PROMPT_MODE: 'append' });
  assert.equal(preset.OCTOCODE_PROMPT_MODE, 'append', 'never overrides an explicit user choice');
});

test('updateCommand: platform self-updates globally, core updates the dependency in place', () => {
  assert.deepEqual(updateCommand('platform'), { cmd: 'npm', args: ['install', '-g', 'octocode-agent@latest'] });
  assert.deepEqual(updateCommand('core'), { cmd: 'npm', args: ['update', CORE_PACKAGE] });
});

test('versionReport names launcher, core, and Pi host', () => {
  const report = versionReport();
  assert.ok(report.includes('octocode-agent'));
  assert.ok(report.includes(CORE_PACKAGE));
  assert.ok(report.includes(PI_PACKAGE));
  assert.equal(report.split('\n')[0].includes(launcherVersion() ?? '?'), true);
});

test('helpReport documents launch, update, and the core-is-the-agent model', () => {
  const h = helpReport();
  assert.ok(h.includes('update'));
  assert.ok(h.includes(CORE_PACKAGE));
});

test('buildPiArgs: always passes --no-extensions to prevent global-extension conflicts', () => {
  assert.deepEqual(
    buildPiArgs('/core', ['chat', '--foo'], {}),
    ['--no-extensions', '--exclude-tools', 'grep,find,ls', '-e', '/core', 'chat', '--foo'],
    'default = no-extensions + lean: prevents globally-installed pi-extension conflicts',
  );
});

test('buildPiArgs: FULL_TOOLS opts out of lean; CLEAN additionally suppresses user skills', () => {
  assert.deepEqual(
    buildPiArgs('/core', ['chat'], { OCTOCODE_AGENT_FULL_TOOLS: '1' }),
    ['--no-extensions', '-e', '/core', 'chat'],
    'FULL_TOOLS keeps grep/find/ls but --no-extensions is still enforced',
  );
  assert.deepEqual(
    buildPiArgs('/core', ['chat'], { OCTOCODE_AGENT_CLEAN: '1', OCTOCODE_AGENT_FULL_TOOLS: '1' }),
    ['--no-extensions', '--no-skills', '-e', '/core', 'chat'],
    'CLEAN adds --no-skills for a fully deterministic branded agent',
  );
});

test('launchAgent execs Pi once with -e <core> + launch env; forwards exit code', () => {
  const calls = [];
  const fakeSpawn = (cmd, args, opts) => {
    calls.push({ cmd, args, env: opts.env });
    return { status: 7 };
  };
  const code = launchAgent(['chat', '--foo'], {
    spawn: fakeSpawn,
    log: () => {},
    resolvePiBin: () => ({ bin: '/pi/bin/pi', pkgRoot: '/pi' }),
    resolveCoreRoot: () => '/core',
    env: { PATH: '/x' },
  });
  assert.equal(code, 7, 'exit code from the pi run is forwarded');
  assert.equal(calls.length, 1, 'single spawn — no global install side effect');
  assert.deepEqual(calls[0].args, ['--no-extensions', '--exclude-tools', 'grep,find,ls', '-e', '/core', 'chat', '--foo']);
  assert.equal(calls[0].env.OCTOCODE_PROMPT_MODE, 'replace');
  assert.equal(calls[0].env.OCTOCODE_AGENT, '1');
});

test('launchAgent honors OCTOCODE_AGENT_EXTENSION_SPEC override', () => {
  const calls = [];
  launchAgent([], {
    spawn: (cmd, args) => (calls.push(args), { status: 0 }),
    log: () => {},
    resolvePiBin: () => ({ bin: '/pi/bin/pi' }),
    resolveCoreRoot: () => '/core',
    env: { OCTOCODE_AGENT_EXTENSION_SPEC: 'npm:@octocodeai/pi-extension@0.3.0' },
  });
  assert.deepEqual(calls[0], ['--no-extensions', '--exclude-tools', 'grep,find,ls', '-e', 'npm:@octocodeai/pi-extension@0.3.0']);
});

test('launchAgent fails cleanly when the Pi host is missing', () => {
  let msg = '';
  const code = launchAgent([], {
    spawn: () => ({ status: 0 }),
    log: (m) => { msg = m; },
    resolvePiBin: () => null,
    resolveCoreRoot: () => '/core',
  });
  assert.equal(code, 1);
  assert.ok(msg.includes('update'));
});

test('main dispatches version/help/update/run', () => {
  let printed = '';
  assert.equal(main(['--version'], { out: (m) => { printed = m; } }), 0);
  assert.ok(printed.includes('octocode-agent'));

  const upd = [];
  assert.equal(main(['update'], { spawn: (c, a) => (upd.push([c, ...a]), { status: 0 }), log: () => {} }), 0);
  assert.deepEqual(upd[0], ['npm', 'install', '-g', 'octocode-agent@latest']);
});
