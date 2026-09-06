import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { runHooksInstall } from '../src/hooks-install-command.js';

import {
  generateOpenCodeAwarenessPlugin,
  installOpenCodeAwarenessPlugin,
  OPENCODE_AWARENESS_PLUGIN_FILE,
} from '../src/opencode-plugin-adapter.js';

describe('OpenCode V2 Awareness plugin adapter', () => {
  it('generates one V2 plugin that routes tool and session events through the bundled runner', () => {
    const source = generateOpenCodeAwarenessPlugin({
      hookRunnerPath: '/opt/octocode/hook-runner.mjs',
      skillRoot: '/opt/octocode/skills/octocode-awareness',
      nodePath: '/usr/bin/node',
    });

    expect(source).toContain('from "@opencode-ai/plugin"');
    expect(source).toContain('id: "octocode.awareness"');
    expect(source).toContain('ctx.tool.hook("execute.before"');
    expect(source).toContain('ctx.tool.hook("execute.after"');
    expect(source).toContain('ctx.event.subscribe({ signal: controller.signal })');
    expect(source).toContain('case "session.created"');
    expect(source).toContain('case "session.compacted"');
    expect(source).toContain('case "session.deleted"');
    expect(source).toContain('[runner, command, "--host", "opencode", "--skill-root", skillRoot]');
    expect(source).toContain('const timeout = setTimeout(');
    expect(source).toContain('child.kill("SIGTERM")');
    expect(source).toContain('code === 2');
    expect(source.match(/hook-runner\.mjs/g)).toHaveLength(1);
    expect(source).not.toContain('octocode-awareness work start');
  });

  it('installs the auto-discovered plugin atomically without changing unrelated OpenCode config', () => {
    const projectDir = mkdtempSync(join(tmpdir(), 'octocode-opencode-plugin-'));
    const hookRunnerPath = resolve(projectDir, 'skill/scripts/hook-runner.mjs');
    const configPath = resolve(projectDir, 'opencode.json');
    const unrelated = '{\n  "$schema": "https://opencode.ai/config.json",\n  "model": "example/model",\n  "plugins": ["existing.plugin"]\n}\n';
    try {
      mkdirSync(resolve(projectDir, 'skill/scripts'), { recursive: true });
      writeFileSync(hookRunnerPath, '#!/usr/bin/env node\n');
      writeFileSync(configPath, unrelated);

      const result = installOpenCodeAwarenessPlugin({
        projectDir,
        hookRunnerPath,
        skillRoot: resolve(projectDir, 'skill'),
        nodePath: process.execPath,
      });

      expect(result).toEqual({
        changed: true,
        pluginPath: resolve(projectDir, '.opencode/plugins', OPENCODE_AWARENESS_PLUGIN_FILE),
      });
      expect(readFileSync(configPath, 'utf8')).toBe(unrelated);
      const installed = readFileSync(result.pluginPath, 'utf8');
      expect(installed).toContain(JSON.stringify(hookRunnerPath));
      expect(installed).toContain(JSON.stringify(resolve(projectDir, 'skill')));

      expect(installOpenCodeAwarenessPlugin({
        projectDir,
        hookRunnerPath,
        skillRoot: resolve(projectDir, 'skill'),
        nodePath: process.execPath,
      })).toEqual({ changed: false, pluginPath: result.pluginPath });
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });

  it('uses the canonical hooks command for dry-run, install, strict check, and owned removal', () => {
    const projectDir = mkdtempSync(join(tmpdir(), 'octocode-opencode-hooks-command-'));
    const skillRoot = resolve(projectDir, 'portable-skill');
    const hookDir = resolve(skillRoot, 'scripts/hooks');
    const runner = resolve(skillRoot, 'scripts/hook-runner.mjs');
    try {
      mkdirSync(hookDir, { recursive: true });
      writeFileSync(runner, '#!/usr/bin/env node\n');

      const dryRun = runHooksInstall(['--host', 'opencode', '--project-dir', projectDir, '--dry-run'], {
        cwd: projectDir,
        hookDir,
      });
      expect(dryRun).toMatchObject({ exitCode: 0, payload: { action: 'dry-run', host: 'opencode', changed: true } });
      const pluginPath = resolve(projectDir, '.opencode/plugins', OPENCODE_AWARENESS_PLUGIN_FILE);
      expect(() => readFileSync(pluginPath, 'utf8')).toThrow();

      expect(runHooksInstall(['--host', 'opencode', '--project-dir', projectDir], { cwd: projectDir, hookDir }))
        .toMatchObject({ exitCode: 0, payload: { action: 'install', host: 'opencode', changed: true } });
      expect(readFileSync(pluginPath, 'utf8')).toContain('id: "octocode.awareness"');
      expect(runHooksInstall(['--host', 'opencode', '--project-dir', projectDir, '--check', '--strict'], { cwd: projectDir, hookDir }))
        .toMatchObject({ exitCode: 0, payload: { ok: true, action: 'check', host: 'opencode' } });
      expect(runHooksInstall(['--host', 'opencode', '--project-dir', projectDir, '--remove'], { cwd: projectDir, hookDir }))
        .toMatchObject({ exitCode: 0, payload: { action: 'remove', host: 'opencode', changed: true } });
      expect(() => readFileSync(pluginPath, 'utf8')).toThrow();
    } finally {
      rmSync(projectDir, { recursive: true, force: true });
    }
  });
});
