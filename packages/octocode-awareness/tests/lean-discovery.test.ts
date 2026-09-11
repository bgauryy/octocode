import { describe, expect, it } from 'vitest';
import { executeAwarenessCli } from '../src/command-cli.js';
import { getAwarenessCommandDescriptor, listAwarenessCommandDescriptors } from '../src/schema/cli.js';
import { AWARENESS_CONCEPTS, ROUTINE_AWARENESS_OPERATIONS } from '../src/schema/operation-catalog.js';

async function schemaCommands(...args: string[]): Promise<Record<string, unknown>> {
  const result = await executeAwarenessCli(['schema', 'commands', ...args, '--compact']);
  expect(result.exitCode, JSON.stringify(result)).toBe(0);
  return result.payload as Record<string, unknown>;
}

describe('lean discovery tiers', () => {
  it('publishes only the five-concept routine map by default', async () => {
    const result = await schemaCommands();
    const concepts = result.concepts as Record<string, string[]>;
    const operations = result.operations as string[];
    expect(Object.keys(concepts)).toEqual(AWARENESS_CONCEPTS);
    expect(operations).toEqual(ROUTINE_AWARENESS_OPERATIONS);
    expect(JSON.stringify(result)).not.toContain('refinement');
    expect(JSON.stringify(result)).not.toContain('session capture');

    const complete = await schemaCommands('--all');
    const operator = complete.operator as Array<{ command: string }>;
    expect(operator.length).toBeGreaterThan(0);
    expect(operator.length).toBeLessThanOrEqual(45);
    expect(operator.map(row => row.command)).not.toEqual(expect.arrayContaining(['refinement get', 'session capture']));
  });

  it('retains exact schemas while keeping compatibility routes out of discovery', async () => {
    const compact = await schemaCommands();
    const complete = await schemaCommands('--all');
    const operator = complete.operator as Array<{ command: string }>;
    expect(operator.map(row => row.command)).toContain('memory evaluate');
    expect(operator.map(row => row.command)).toContain('history recovery');
    expect(listAwarenessCommandDescriptors().some(row => row.command === 'session capture')).toBe(true);

    const exact = await executeAwarenessCli(['schema', 'command', 'memory', 'evaluate', '--compact']);
    expect(exact.exitCode).toBe(0);
    expect(exact.payload).toMatchObject({ 'x-cli-command': 'memory evaluate' });
    const canonical = await executeAwarenessCli(['schema', 'command', 'work', 'protect', '--compact']);
    expect(canonical.payload).toMatchObject({ 'x-awareness-operation': 'work.protect' });
    expect(compact).toMatchObject({ ok: true, hint: expect.stringContaining('directly') });
  });

  it('documents every detail trigger in the attend schema', () => {
    const descriptor = getAwarenessCommandDescriptor('attend');
    const details = JSON.stringify(descriptor?.inputSchema.properties);
    for (const trigger of ['query', 'file', 'artifact', 'repo', 'ref', 'include_bodies', 'explain_organ', 'revision']) {
      expect(details).toContain(trigger);
    }
    expect(JSON.stringify(descriptor?.inputSchema)).toContain('changes selects the separate Git view');
  });
});
