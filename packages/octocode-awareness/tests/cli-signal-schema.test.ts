import { describe, expect, it } from 'vitest';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { z } from 'zod';
import { schemas } from '../src/schema/registry.js';
import { cliAllowedFlags, projectCliProperties } from '../src/schema/cli-contract.js';

describe('signal CLI schema routing fields', () => {
  for (const command of ['signal publish', 'signal reply']) {
    it(`${command} exposes repeatable recipients and reference IDs`, () => {
      const schema = z.toJSONSchema(schemas.agent_signal);
      const properties = structuredClone(schema.properties!) as Record<string, unknown>;
      projectCliProperties(properties, command);
      expect(properties.to_agent).toMatchObject({ type: 'array', items: { type: 'string' } });
      expect(properties.ref_id).toMatchObject({ type: 'array', items: { type: 'string' } });
      expect(properties).not.toHaveProperty('to_agents');
      expect(properties).not.toHaveProperty('refs');
    });
  }
});

describe('task CLI action contracts', () => {
  it('places the run test plan and lease on claim, not create', () => {
    expect(cliAllowedFlags('task create')).not.toContain('test_plan');
    expect(cliAllowedFlags('task create')).not.toContain('lease_minutes');
    expect(cliAllowedFlags('task claim')).toContain('test_plan');
    const properties = structuredClone(z.toJSONSchema(schemas.task).properties!) as Record<string, unknown>;
    projectCliProperties(properties, 'task claim');
    expect(properties.test_plan).toMatchObject({ type: 'string' });
  });
  it('includes the supported retry action in the shared schema', () => {
    expect(schemas.task.safeParse({ action: 'retry', task_id: 'task_fixture', agent_id: 'reviewer' }).success).toBe(true);
  });
  it('rejects ignored create-time run options before any task is created', () => {
    const root = mkdtempSync(join(tmpdir(), 'aw-create-options-'));
    try {
      for (const option of [['--test-plan', 'node --version'], ['--lease-minutes', '10']]) {
        const result = spawnSync(process.execPath, [resolve(import.meta.dirname, '../out/octocode-awareness.js'), 'task', 'create', '--db', join(root, 'aw.sqlite3'), '--agent-id', 'lead', '--plan-id', 'missing-plan', '--title', 'Fixture', '--reasoning', 'Check options', '--acceptance', 'Check runs', '--path', 'fixture.ts', ...option], {
          cwd: root, env: { ...process.env, OCTOCODE_HOME: join(root, 'home') }, encoding: 'utf8', timeout: 10000,
        });
        expect(result.status).toBe(1);
        expect(result.stdout).toContain('task claim');
      }
    } finally { rmSync(root, { recursive: true, force: true }); }
  });
});

describe('CLI-only discovery fields', () => {
  for (const [command, schema] of [['plan list', schemas.plan], ['task list', schemas.task], ['task ready', schemas.task]] as const) {
    it(`${command} exposes its supported row limit and full output flag`, () => {
      const properties = structuredClone(z.toJSONSchema(schema).properties!) as Record<string, unknown>;
      projectCliProperties(properties, command);
      expect(properties.limit).toMatchObject({ type: 'integer', maximum: 200 });
      expect(properties.full).toMatchObject({ type: 'boolean' });
    });
  }
  it('signal list exposes repeated kinds and the all-read-states switch', () => {
    const properties = structuredClone(z.toJSONSchema(schemas.agent_signal).properties!) as Record<string, unknown>;
    projectCliProperties(properties, 'signal list');
    expect(properties.kind).toMatchObject({ type: 'array' });
    expect(properties.all).toMatchObject({ type: 'boolean' });
  });
});
