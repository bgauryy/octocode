import { describe, expect, it } from 'vitest';
import { join } from 'node:path';
import {
  AGENT_DB_FILENAME,
  OCTOCODE_AGENT_DB_PATH_ENV,
  agentDbPath,
  agentHome,
  getOctocodeHome,
  safeSessionId,
  sessionArtifactDir,
  sessionDir,
  sessionsRoot,
} from '../src/paths.js';

const HOME = '/tmp/octo-home';
const env = (extra: NodeJS.ProcessEnv = {}): NodeJS.ProcessEnv => ({ OCTOCODE_HOME: HOME, ...extra });

describe('getOctocodeHome', () => {
  it('remains the product home even when the agent root is overridden', () => {
    expect(getOctocodeHome(env({ OCTOCODE_AGENT_DIR: '/tmp/custom-agent' }))).toBe(HOME);
  });
});

describe('agentHome', () => {
  it('defaults to <home>/agent', () => {
    expect(agentHome(env())).toBe(join(HOME, 'agent'));
  });

  it('honors the agent-root override without changing the product home', () => {
    const overridden = env({ OCTOCODE_AGENT_DIR: '/tmp/custom-agent' });
    expect(agentHome(overridden)).toBe('/tmp/custom-agent');
    expect(getOctocodeHome(overridden)).toBe(HOME);
  });
});

describe('agentDbPath', () => {
  it('defaults to <home>/agent/agent.sqlite3', () => {
    expect(agentDbPath(env())).toBe(join(HOME, 'agent', AGENT_DB_FILENAME));
  });

  it('prefers the agent-specific DB override', () => {
    expect(agentDbPath(env({ [OCTOCODE_AGENT_DB_PATH_ENV]: '/var/db/agent.sqlite3' }))).toBe('/var/db/agent.sqlite3');
  });

  it('ignores blank DB overrides', () => {
    expect(agentDbPath(env({
      [OCTOCODE_AGENT_DB_PATH_ENV]: '   ',
    }))).toBe(join(HOME, 'agent', AGENT_DB_FILENAME));
  });
});

describe('session paths', () => {
  it('roots sessions under <home>/agent/sessions', () => {
    expect(sessionsRoot(env())).toBe(join(HOME, 'agent', 'sessions'));
  });

  it('builds a per-session directory', () => {
    expect(sessionDir('abc123', env())).toBe(join(HOME, 'agent', 'sessions', 'abc123'));
  });

  it('builds artifact bucket directories', () => {
    expect(sessionArtifactDir('abc123', 'compaction', env())).toBe(
      join(HOME, 'agent', 'sessions', 'abc123', 'compaction'),
    );
  });
});

describe('safeSessionId', () => {
  it('sanitizes unsafe characters', () => {
    expect(safeSessionId('a/b c:d')).toBe('a_b_c_d');
  });

  it('falls back to a pid-scoped id when empty', () => {
    expect(safeSessionId('   ')).toBe(`pid-${process.pid}`);
    expect(safeSessionId(null)).toBe(`pid-${process.pid}`);
  });

  it('caps very long ids', () => {
    expect(safeSessionId('x'.repeat(200))).toHaveLength(96);
  });
});
