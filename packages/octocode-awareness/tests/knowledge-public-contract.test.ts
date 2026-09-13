import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import { getAwarenessOperationDescriptor } from '../src/schema/operation-catalog.js';
import { executeAwarenessCli } from '../src/command-cli.js';

describe('experience and knowledge public discovery', () => {
  it.each(['memory.set', 'memory.get', 'memory.revalidate', 'history.experience'])(
    'exposes one executable CLI/API contract for %s', async operation => {
      const descriptor = getAwarenessOperationDescriptor(operation);
      expect(descriptor, operation).toBeDefined();
      expect(() => z.fromJSONSchema(descriptor!.inputSchema)).not.toThrow();
      const discovery = await executeAwarenessCli(['schema', 'command', ...operation.split('.')]);
      expect(discovery.exitCode).toBe(0);
      expect(discovery.payload).toMatchObject({ 'x-awareness-operation': operation });
    },
  );

  it('keeps bound identity out of caller-controlled knowledge inputs', () => {
    const descriptor = getAwarenessOperationDescriptor('memory.get');
    expect(descriptor).toBeDefined();
    expect(() => descriptor!.validate({ key: 'lesson', workspace: '/another-repo' })).toThrow();
  });

  it('accepts relevant flow and failure signals in orientation', () => {
    const descriptor = getAwarenessOperationDescriptor('context.orient')!;
    const params = { flow: 'message.reply', failure_signature: 'reply-schema-mismatch' };
    expect(() => descriptor.validate(params)).not.toThrow();
    expect(z.fromJSONSchema(descriptor.inputSchema).safeParse(params).success).toBe(true);
  });
});
