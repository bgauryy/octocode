import { existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

import { resourceLeads } from '../src/attend-model.js';

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');

describe('canonical orientation resource leads', () => {
  it('returns only current, existing architecture sources', () => {
    const leads = resourceLeads('awareness architecture and memory', REPO_ROOT);
    const generatedRfc = resolve(
      REPO_ROOT,
      '.octocode/rfc/awareness-one-surface/RFC.md'
    );
    const architecture = resolve(
      REPO_ROOT,
      'packages/octocode-awareness/ARCHITECTURE.md'
    );

    expect(leads.map(({ source }) => source)).toEqual([
      ...(existsSync(generatedRfc) ? [generatedRfc] : []),
      architecture,
    ]);
    expect(existsSync(architecture)).toBe(true);
    expect(leads.every(({ source }) => typeof source === 'string' && existsSync(source))).toBe(true);
    expect(JSON.stringify(leads)).not.toMatch(/homeostatic|self-reflection/);
  });

  it('does not emit speculative paths for a workspace without guidance files', () => {
    expect(resourceLeads('unrelated work', '/definitely/missing/workspace')).toEqual([]);
  });
});
