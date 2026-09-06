import assert from 'node:assert/strict';
import { test } from 'vitest';
import {
  collectPublicCommands,
  formatCommandsGuide,
} from '../src/tools/commands-command.js';
import type { PiCommand, PiCommandContext, PiInstance } from '../src/types.js';

function command(
  name: string,
  description: string | undefined,
  source: PiCommand['source'] = 'extension',
): PiCommand {
  return {
    name,
    description,
    source,
    sourceInfo: {
      path: '/test',
      source: 'test',
      scope: 'temporary',
      origin: 'top-level',
    },
  };
}

test('collectPublicCommands returns the live deduplicated registry and excludes private commands', () => {
  const commands = [
    command('octocode-now', 'Show live work'),
    command('_octocode-clear-context-impl', 'private'),
    command('model', 'Select model'),
    command('octocode-now', 'Latest description wins'),
  ];
  const pi = { getCommands: () => commands } as unknown as PiInstance;

  assert.deepEqual(
    collectPublicCommands(pi).map(({ name, description }) => ({ name, description })),
    [
      { name: 'model', description: 'Select model' },
      { name: 'octocode-now', description: 'Latest description wins' },
    ],
  );

  commands.push(command('skill:review', 'Run the review skill', 'skill'));
  assert.deepEqual(
    collectPublicCommands(pi).map((item) => item.name),
    ['model', 'octocode-now', 'skill:review'],
    'each invocation reads the current registry rather than a startup snapshot',
  );
});

test('formatCommandsGuide groups Octocode, Pi, and skill/template commands with use guidance', () => {
  const guide = formatCommandsGuide([
    command('model', 'Select the active model'),
    command('octocode-now', 'Show live work'),
    command('commands', 'List every command'),
    command('skill:review', 'Run review workflow', 'skill'),
    command('deploy', undefined, 'prompt'),
  ], { status: 'missing' });

  assert.match(guide, /^◆ Commands — live slash-command guide/m);
  assert.match(guide, /Octocode commands[\s\S]*\/commands — Use when: List every command/);
  assert.match(guide, /Pi and extension commands[\s\S]*\/model — Use when: Select the active model/);
  assert.match(guide, /Skills and templates[\s\S]*\/skill:review — Use when: Run review workflow/);
  assert.match(guide, /\/deploy — Use when: No description provided\./);
  assert.match(guide, /GitHub: login required/);
  assert.match(guide, /npx octocode auth login/);
  assert.match(guide, /gh auth login/);
});
