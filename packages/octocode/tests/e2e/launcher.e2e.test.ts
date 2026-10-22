import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { binary, launcher, ready } from './native-fixture.js';

// End-to-end: the BUILT launcher against the REAL packaged native binary —
// the exact path `npx octocode` takes. Everything else in this suite mocks
// the delegation boundary; this file is the one place the boundary is real.
// Skips when either artifact is absent (unbuilt checkout, platforms without
// committed binaries).

function runLauncher(args: string[], env: NodeJS.ProcessEnv = {}) {
  return spawnSync(process.execPath, [launcher, ...args], {
    encoding: 'utf8',
    env: { ...process.env, NO_COLOR: '1', ...env },
    timeout: 60_000,
  });
}

describe.skipIf(!ready)('launcher → native binary e2e', () => {
  it.each([
    [],
    ['--help'],
    ['--version'],
    ['--version', '--bogus'],
    ['--', '--help'],
    ['unknown', '--json'],
    ['schema'],
    ['schema', 'localFetch', '--view', 'query'],
    ['config', '--json'],
    ['config', 'view', '--help'],
    ['config', 'view', '--idle-timeout=abc'],
    ['config', 'view', '--json', '--idle-timeout=abc'],
    ['config', 'view', '--idle-timeout=0'],
    ['config', 'view', '--idle-timeout=3601'],
    ['config', 'view', '--add', 'KEY'],
    ['install'],
    ['install', '--json'],
    ['install', '--list', '--json'],
    ['install', '--ide', 'not-a-client', '--json'],
    ['skill', 'list', '--json'],
    ['skill', 'info', 'octocode-architect', '--json'],
    ['localSearch', '{"queries":['],
    ['localFetch', '{"queries":[{"path":"package.json","ranges":["1-3"]}]}'],
  ])('matches native stdout, stderr, and exit status for %j', (...args) => {
    const native = spawnSync(binary as string, args, {
      encoding: 'utf8',
      env: { ...process.env, NO_COLOR: '1' },
      timeout: 60_000,
    });
    const node = runLauncher(args);
    expect(native.error).toBeUndefined();
    expect(node.error).toBeUndefined();
    expect(node.status).toBe(native.status);
    expect(node.stdout).toBe(native.stdout);
    expect(node.stderr).toBe(native.stderr);
  });

  it.each([
    ['beta off, no key', '0', ''],
    ['beta off, key present', '0', 'discovery-only-test-key'],
    ['beta on, no key', '1', ''],
  ])(
    'serves the schema catalog with %s and omits gated clasify',
    (_case, beta, key) => {
      const env = {
        OCTOCODE_BETA: beta,
        OCTOCODE_CLASSIFICATION_API: key,
        OCTOCODE_CLASSIFICATION_API_HOST: 'http://127.0.0.1:9',
      };
      const main = runLauncher([], env);
      const schema = runLauncher(['schema'], env);
      expect(main.status).toBe(0);
      expect(schema.status).toBe(0);
      expect(JSON.parse(main.stdout)).toEqual(JSON.parse(schema.stdout));
      expect(JSON.parse(schema.stdout).instructions).toBeUndefined();
      expect(JSON.parse(schema.stdout).commands.full).toBe(
        'octocode schema --view full'
      );
      const full = runLauncher(['schema', '--view', 'full'], env);
      expect(full.status).toBe(0);
      const instructions = JSON.parse(full.stdout).instructions as string;
      expect(instructions.length).toBeGreaterThan(0);
      expect(
        JSON.parse(full.stdout).tools.map((tool: { name: string }) => tool.name)
      ).toEqual(
        JSON.parse(schema.stdout).tools.map(
          (tool: { name: string }) => tool.name
        )
      );
      expect(instructions.includes('<clasify>')).toBe(Boolean(key));
      expect(instructions.includes('clasifyContext')).toBe(Boolean(key));
      expect(instructions).not.toContain('clasify scouts unread');
      expect(
        JSON.parse(schema.stdout).tools.map(
          (tool: { name: string }) => tool.name
        )
      ).not.toContain('clasify');
    }
  );

  it('prints native root help without agent instructions', () => {
    for (const args of [['--help'], ['-h'], ['help']]) {
      const help = runLauncher(args);
      expect(help.status).toBe(0);
      expect(help.stdout).toMatch(/^\s+schema\s/m);
      expect(help.stdout).not.toContain('Agent instructions');
      expect(help.stdout).not.toMatch(/\bscheme\b|showConfig|--json-errors/);
    }
    const schemaHelp = runLauncher(['schema', '--help']);
    expect(schemaHelp.status).toBe(0);
    expect(schemaHelp.stdout).toContain('Usage: octocode schema');
  });

  it('reports errors as the JSON envelope with --json', () => {
    const typo = runLauncher(['lokalSearch', '{}', '--json']);
    expect(typo.status).toBe(2);
    // The envelope is the error itself: no kind/version/tool frame.
    const envelope = JSON.parse(typo.stdout);
    expect(envelope.error).toContain('lokalSearch');
    expect(envelope).not.toHaveProperty('kind');
    const badJson = runLauncher(['localSearch', '{"queries":[', '--json']);
    expect(badJson.status).toBe(2);
    expect(JSON.parse(badJson.stdout).error).toContain('Invalid JSON query');
  });

  it('redacts native argument errors on both output channels', () => {
    const token = `ghp_${'a'.repeat(37)}`;
    const json = runLauncher(['localFetch', '{}', token, '--json']);
    expect(json.status).toBe(2);
    expect(json.stderr).toBe('');
    expect(json.stdout).not.toContain(token);
    const error = JSON.parse(json.stdout);
    expect(error.errorCode).toBe('invalidInput');
    expect(error.error).toMatch(/^unexpected argument '\[REDACTED-/);
    expect(error.error).toContain("' found; a JSON query is the only argument");
    expect(error).not.toHaveProperty('kind');

    const text = runLauncher(['config', token]);
    expect(text.status).toBe(2);
    expect(text.stdout).toBe('');
    expect(text.stderr).not.toContain(token);
    expect(text.stderr).toMatch(/^error: unrecognized subcommand '\[REDACTED-/);
    expect(text.stderr).toContain('\n\nUsage: octocode config [OPTIONS]\n');
    expect(text.stderr).toMatch(/For more information, try '--help'\.\n$/);
  });

  it('reads the query from stdin with --input -', () => {
    const result = spawnSync(
      process.execPath,
      [launcher, 'localFetch', '--input', '-', '--json'],
      {
        encoding: 'utf8',
        env: { ...process.env, NO_COLOR: '1' },
        input: JSON.stringify({
          queries: [{ path: launcher, ranges: ['1-1'] }],
        }),
        timeout: 60_000,
      }
    );
    expect(result.status, result.stderr).toBe(0);
    expect(JSON.parse(result.stdout).results[0].index).toBe(0);
  });

  it('serves --version as the one package version', () => {
    const result = runLauncher(['--version']);
    expect(result.status).toBe(0);
    expect(result.stdout).toBe(
      `octocode ${JSON.parse(readFileSync(resolve(__dirname, '..', '..', 'package.json'), 'utf8')).version}\n`
    );
  });

  it('executes a real tool call end-to-end with exit 0 and structured JSON', () => {
    const query = JSON.stringify({
      queries: [
        {
          path: resolve(__dirname, '..', '..', 'src'),
          matchString: 'delegateToNative',
          pageSize: 5,
        },
      ],
    });
    const result = runLauncher(['localSearch', query, '--json']);
    expect(result.status).toBe(0);
    const payload = JSON.parse(result.stdout) as {
      results: Array<{ index: number }>;
    };
    expect(payload.results[0].index).toBe(0);
  });

  it('propagates native validation failures as exit 2', () => {
    const result = runLauncher(['localSearch', '{"path":"/tmp"}']);
    expect(result.status).toBe(2);
  });

  it('ships the same skill catalog through native and Node entrypoints', () => {
    const native = spawnSync(binary as string, ['skill', 'list', '--json'], {
      encoding: 'utf8',
      env: { ...process.env, PATH: '' },
      timeout: 30_000,
    });
    const node = runLauncher(['skill', 'list', '--json']);
    expect(native.status, native.stderr).toBe(0);
    expect(node.status, node.stderr).toBe(0);
    const names = (stdout: string) =>
      JSON.parse(stdout).skills.map((skill: { name: string }) => skill.name);
    expect(names(native.stdout)).toEqual(names(node.stdout));
    expect(names(native.stdout)).toContain('octocode-architect');
  });
});
