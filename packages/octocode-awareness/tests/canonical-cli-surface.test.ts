import { afterEach, describe, expect, it } from 'vitest';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { executeAwarenessCli } from '../src/command-cli.js';
import { HELP, HELP_COMPACT } from '../src/cli-adapter/cli-help-data.js';
import {
  AWARENESS_CONCEPTS,
  ROUTINE_AWARENESS_OPERATIONS,
} from '../src/schema/operation-catalog.js';

const roots: string[] = [];
afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

function workspace(): { root: string; db: string } {
  const root = mkdtempSync(join(tmpdir(), 'awareness-canonical-cli-'));
  roots.push(root);
  return { root, db: join(root, 'awareness.sqlite3') };
}

describe('canonical Awareness CLI surface', () => {
  it('discovers exactly five concepts and nineteen directly callable operations', async () => {
    const result = await executeAwarenessCli(['schema', 'commands', '--compact']);
    expect(result.exitCode, JSON.stringify(result)).toBe(0);
    const payload = result.payload as {
      concepts: Record<string, string[]>;
      operations: string[];
      call: string;
      schema: string;
    };
    expect(Object.keys(payload.concepts)).toEqual(AWARENESS_CONCEPTS);
    expect(payload.operations).toEqual(ROUTINE_AWARENESS_OPERATIONS);
    expect(payload.operations).toHaveLength(19);
    expect(payload.call).toBe('<concept> <operation> [flags]');
    expect(payload.schema).toBe('schema command <concept> <operation>');
    expect(Buffer.byteLength(JSON.stringify(payload))).toBeLessThanOrEqual(1_500);
    for (const operation of payload.operations) {
      const direct = await executeAwarenessCli([
        'schema', 'command', ...operation.split('.'), '--compact',
      ]);
      expect(direct.exitCode, `${operation}: ${JSON.stringify(direct.payload)}`).toBe(0);
      expect(direct.payload).toMatchObject({ 'x-awareness-operation': operation });
    }
  });

  it('keeps explicit operator and recovery discovery bounded and separate', async () => {
    const result = await executeAwarenessCli(['schema', 'commands', '--all', '--compact']);
    expect(result.exitCode, JSON.stringify(result)).toBe(0);
    const payload = result.payload as { operations: string[]; operator: Array<{ command: string }> };
    expect(payload.operations).toHaveLength(19);
    expect(payload.operator.length).toBeLessThanOrEqual(45);
    expect(payload.operator.map(row => row.command)).not.toEqual(expect.arrayContaining([
      'attend', 'status', 'query', 'agent list', 'refinement get', 'session capture', 'hook run',
    ]));
    expect(payload.operator.map(row => row.command)).toEqual(expect.arrayContaining([
      'database consolidate', 'maintenance digest', 'hooks install', 'schema entities',
    ]));
  });

  it('returns canonical direct-call schemas and executes canonical message routes', async () => {
    const schema = await executeAwarenessCli(['schema', 'command', 'message', 'send', '--compact']);
    expect(schema.exitCode, JSON.stringify(schema)).toBe(0);
    expect(schema.payload).toMatchObject({
      'x-awareness-operation': 'message.send',
      'x-cli-command': 'message send',
      properties: { kind: expect.any(Object), subject: expect.any(Object) },
    });

    const { root, db } = workspace();
    const sent = await executeAwarenessCli([
      '--db', db, 'message', 'send', '--workspace', root, '--agent-id', 'sender',
      '--kind', 'fyi', '--subject', 'Canonical route', '--to-agent', 'peer', '--compact',
    ]);
    expect(sent.exitCode, JSON.stringify(sent)).toBe(0);
    const listed = await executeAwarenessCli([
      '--db', db, 'message', 'list', '--workspace', root, '--agent-id', 'peer',
      '--include-bodies', '--compact',
    ]);
    expect(listed.exitCode, JSON.stringify(listed)).toBe(0);
    expect(JSON.stringify(listed.payload)).toContain('Canonical route');
  });

  it('teaches one five-concept surface without advertising legacy nouns', async () => {
    for (const help of [HELP, HELP_COMPACT]) {
      for (const concept of AWARENESS_CONCEPTS) expect(help.toLowerCase()).toContain(concept);
      for (const legacy of ['refinement', 'agent register', 'agent list', 'query workboard']) {
        expect(help.toLowerCase()).not.toContain(legacy);
      }
    }
    const help = await executeAwarenessCli(['work', 'protect', '--help', '--compact']);
    expect(help.exitCode).toBe(0);
    expect(help.text).toContain('work protect');
    expect(help.text).toContain('--action');
    expect(help.text).not.toContain('schema command');
    const compactRoot = await executeAwarenessCli(['--compact']);
    expect(compactRoot).toMatchObject({ exitCode: 0, text: HELP_COMPACT });
  });
});
