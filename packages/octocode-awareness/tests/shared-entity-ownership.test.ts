import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const PACKAGE_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

function source(relativePath: string): string {
  return readFileSync(resolve(PACKAGE_ROOT, relativePath), 'utf8');
}

describe('shared entity ownership', () => {
  it('uses shared plan records instead of redeclaring durable record shapes', () => {
    const plans = source('src/plans.ts');
    const tasks = source('src/tasks-catalog.ts');

    expect(plans).toContain("from '@octocodeai/agent-contracts/entities'");
    expect(tasks).toContain("from '@octocodeai/agent-contracts/entities'");
    expect(plans).not.toMatch(/(?:interface|type)\s+PlanRecord\b/);
    expect(tasks).not.toMatch(/(?:interface|type)\s+PlanTaskRecord\b/);
  });
});
