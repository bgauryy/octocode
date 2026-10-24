import fs from 'node:fs';
import path from 'node:path';
import { afterAll, beforeAll } from 'vitest';
import { registerAgentTool } from '../src/subagents/tool.js';
import type { AgentProfile } from '../src/subagents/profiles.js';
import { Subcommands } from '../src/shared/commands.js';
import { Team } from '../src/team/session.js';
import { fakeCtx, fakePi, fakePermission } from './fake-pi.js';
import { git, tmp } from './helpers.js';

const FIXTURE = path.join(path.dirname(new URL(import.meta.url).pathname), 'subagent-fake-pi.cjs');

export function repo(): string {
  const dir = fs.realpathSync(tmp('octocode-agent-repo-'));
  git(dir, 'init', '-q', '-b', 'main');
  git(dir, 'config', 'user.name', 'Test');
  git(dir, 'config', 'user.email', 'test@example.com');
  fs.writeFileSync(path.join(dir, 'a.txt'), 'one\n');
  git(dir, 'add', '-A');
  git(dir, 'commit', '-q', '-m', 'init');
  return dir;
}

/**
 * Runs the file's subagents with the scripted `pi` stand-in, under a fresh Octocode home and team database plus `env`.
 * `setup-env.ts` already cleared any inherited OCTOCODE_* identity.
 */
export function useScriptedPi(env: Record<string, string> = {}): void {
  const savedArgv = process.argv[1];
  const savedEnv = { ...process.env };
  beforeAll(() => {
    const root = tmp('octocode-agent-tool-');
    Object.assign(process.env, { OCTOCODE_HOME: path.join(root, 'home'), OCTOCODE_AGENT_DB: path.join(root, 'team', 'team.sqlite'), ...env });
    // runSubagent re-runs the current `pi` script: point it at the scripted stand-in.
    const bin = path.join(root, 'bin');
    fs.mkdirSync(bin);
    fs.copyFileSync(FIXTURE, path.join(bin, 'pi'));
    process.argv[1] = path.join(bin, 'pi');
  });

  afterAll(() => {
    process.argv[1] = savedArgv!;
    process.env = savedEnv;
  });
}

export const profiles = new Map<string, AgentProfile>([
  ['researcher', { name: 'researcher', description: 'reads', prompt: 'Research only.', tools: 'read,bash', excludeTools: 'file', model: 'faux/model' }],
  ['webLive', { name: 'webLive', description: 'browser', prompt: 'Drive the browser.', visibleBrowser: true }],
]);

/** Why profile files were refused when the profiles loaded, by file name (as `index.ts` collects them from `onSkip`). */
export const refused = new Map<string, string>();

export function setup(cwd = tmp('octocode-agent-cwd-'), hasUI = true) {
  const f = fakePi();
  const team = new Team(f.pi);
  const control = registerAgentTool(f.pi, () => profiles, team, fakePermission(), () => refused);
  // As index.ts wires it: `/octocode agents`, with the `/agents` shortcut.
  const commands = new Subcommands();
  commands.add('agents', control.command);
  commands.alias(f.pi, 'agents');
  const ctx = fakeCtx({ cwd, hasUI });
  // The parent's models: `faux/model` is the session's, `faux/other` and `alt/shared` are also available.
  const models = [{ provider: 'faux', id: 'model' }, { provider: 'faux', id: 'other' }, { provider: 'alt', id: 'shared' }];
  Object.assign(ctx, { model: models[0], modelRegistry: { getAvailable: () => models, find: (provider: string, id: string) => models.find((m) => m.provider === provider && m.id === id) } });
  team.start(ctx);
  const agent = f.tools.get('agent');
  const run = (params: Record<string, unknown>, signal?: AbortSignal, onUpdate?: (update: unknown) => void) => agent.execute('call-1', { goal: 'check things', ...params }, signal, onUpdate, ctx);
  return { ...f, team, ctx, agent, run, cwd, control };
}

export const reportOf = (text: string) => JSON.parse(text.slice(text.indexOf('{'), text.indexOf('}\n') >= 0 ? text.indexOf('}\n') + 1 : text.lastIndexOf('}') + 1));
