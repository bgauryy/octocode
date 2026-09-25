import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {
  buildSurfaceSpec,
  loadProfile,
  profileToPiArgs,
} from '../src/surfaces.js';

describe('buildSurfaceSpec — external octocode CLI', () => {
  it('tools forwards the current direct CLI grammar without a retired subcommand', () => {
    expect(buildSurfaceSpec('tools', ['scheme', 'localFetch', '--view', 'query', '--compact'])).toEqual({
      cmd: 'npx',
      args: ['octocode', 'scheme', 'localFetch', '--view', 'query', '--compact'],
    });
  });

  it('skills maps to `npx octocode skill`', () => {
    expect(buildSurfaceSpec('skills', ['--list'])).toEqual({
      cmd: 'npx',
      args: ['octocode', 'skill', '--list'],
    });
  });
});


describe('profiles', () => {
  let home: string;

  beforeEach(() => {
    home = fs.mkdtempSync(path.join(os.tmpdir(), 'oca-prof-'));
  });

  afterEach(() => {
    fs.rmSync(home, { recursive: true, force: true });
  });

  it('loads a named profile and translates it to Pi flags', () => {
    fs.writeFileSync(
      path.join(home, 'profiles.json'),
      JSON.stringify({
        ci: {
          model: 'anthropic/claude-sonnet-4-5',
          excludeTools: 'spawnAgent',
          approve: 'always',
        },
      }),
    );
    const profile = loadProfile('ci', home);
    expect(profile).not.toBeNull();
    expect(profileToPiArgs(profile!)).toEqual([
      '--model',
      'anthropic/claude-sonnet-4-5',
      '--exclude-tools',
      'spawnAgent',
      '-a',
    ]);
  });

  it('returns null for a missing file or unknown profile', () => {
    expect(loadProfile('x', home)).toBeNull();
    fs.writeFileSync(path.join(home, 'profiles.json'), JSON.stringify({ fast: {} }));
    expect(loadProfile('missing', home)).toBeNull();
  });

  it('maps approve:never to -na', () => {
    expect(profileToPiArgs({ approve: 'never' })).toEqual(['-na']);
  });
});
