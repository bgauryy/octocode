import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { openAwarenessStore } from '../src/coordination/open.js';
import { AWARENESS_CONCEPTS, listAwarenessOperationDescriptors } from '../src/schema/operation-catalog.js';

const CANONICAL_COMMANDS = Object.fromEntries(AWARENESS_CONCEPTS.map(concept => [
  concept,
  listAwarenessOperationDescriptors().filter(row => row.concept === concept).map(row => row.operation.slice(concept.length + 1)),
]));

let root: string;
let store: ReturnType<typeof openAwarenessStore>;

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), 'aw-schema-cmd-'));
  store = openAwarenessStore({ workspace: root, dbPath: join(root, 'aw.sqlite3') });
});

afterEach(() => {
  store.close();
  rmSync(root, { recursive: true, force: true });
});

describe('schemaCommand', () => {
  it('returns the full command map by default and for "commands"', () => {
    const all = store.schemaCommand() as Record<string, string[]>;
    expect(all).toEqual(store.schemaCommand('commands'));
    expect(Object.keys(all)).toEqual(Object.keys(CANONICAL_COMMANDS));
  });

  it('lists nouns for "list" and resolves one noun to its actions', () => {
    const nouns = store.schemaCommand('list') as string[];
    expect(nouns).toEqual(Object.keys(CANONICAL_COMMANDS));
    const noun = nouns[0]!;
    expect(store.schemaCommand(noun)).toEqual({
      command: noun,
      actions: CANONICAL_COMMANDS[noun],
    });
  });

  it('rejects an unknown noun', () => {
    expect(() => store.schemaCommand('no-such-noun')).toThrow(/unknown schema command/);
  });
});
