import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import { schemas } from '../src/schema/registry.js';
import { cliAllowedFlags, projectCliProperties } from '../src/schema/cli-contract.js';
import { projectCommandInput } from '../src/schema/command-input.js';

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

describe('projected conditional command contracts', () => {
  it.each([
    ['history recovery', 'history_recovery', { workspace: '/repo', action: 'reconcile' }, { workspace: '/repo', action: 'reconcile', confirm: 'reconcile' }],
    ['history evidence', 'history_evidence', { workspace: '/repo', action: 'reclaim' }, { workspace: '/repo', action: 'reclaim', confirm: 'reclaim' }],
  ] as const)('%s requires its explicit mutation confirmation', (command, schemaName, invalid, valid) => {
    const projected = z.fromJSONSchema(projectCommandInput(command, schemas[schemaName]));
    expect(projected.safeParse(invalid).success).toBe(false);
    expect(projected.safeParse(valid).success).toBe(true);
    expect(projected.safeParse({ workspace: '/repo' }).success).toBe(true);
  });
});
