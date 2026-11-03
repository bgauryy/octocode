/** The command catalog, assembled from its Zod sources and rendered as `assets/catalog.json` (the runtime's one data file). */
import { z } from 'zod';
import { commandGroups } from './commands.ts';
import { recordEnvelopeSchema } from './defs.ts';
import { catalogSchema, dropImplicitBounds } from './json-schema.ts';
import { definitionIds, ref, registry } from './registry.ts';
import { recordTypes } from './record-types.ts';
import { tools } from './tools.ts';

type Json = Record<string, unknown>;

const commands = commandGroups.flat();

/** Registers every command input, result and record payload under its own id, then converts the whole registry once. */
function schemas(): Record<string, Json> {
  for (const command of commands) {
    if (command.input) registry.add(command.input, { id: `input/${command.name}` });
    registry.add(command.result, { id: `result/${command.name}` });
  }
  for (const item of recordTypes) registry.add(item.dataSchema, { id: `data/${item.type}` });
  registry.add(ref(recordEnvelopeSchema), { id: 'envelope' });
  // `io: 'input'` keeps defaulted fields optional: the catalog describes what a caller may send.
  const converted = z.toJSONSchema(registry, { uri: id => `#/$defs/${id}`, io: 'input', unrepresentable: 'any', override: dropImplicitBounds }).schemas;
  return Object.fromEntries(Object.entries(converted).map(([id, schema]) => [id, catalogSchema(schema)]));
}

/** The catalog as the runtime reads it. */
export function buildCatalog(): Json {
  const converted = schemas();
  const at = (id: string): Json => {
    const schema = converted[id];
    if (!schema) throw Error(`catalog schema ${id} was not generated`);
    return schema;
  };
  return {
    commands: commands.map(command => ({
      name: command.name,
      description: command.description,
      ...(command.input && { inputSchema: at(`input/${command.name}`) }),
      ...(command.annotations && { annotations: command.annotations }),
      usage: command.usage,
      cliFlags: command.cliFlags,
      ...(command.cliSchema && { cliSchema: command.cliSchema }),
    })),
    tools,
    $defs: Object.fromEntries(definitionIds.map(id => [id, at(id)])),
    recordTypes: recordTypes.map(item => ({ type: item.type, writer: item.writer, description: item.description, dataSchema: at(`data/${item.type}`), example: item.example, routing: item.routing, fetch: item.fetch })),
    recordEnvelope: at('envelope'),
    results: Object.fromEntries(commands.map(command => [command.name, at(`result/${command.name}`)])),
  };
}

/** The exact text of `assets/catalog.json`. */
export const renderCatalog = (): string => `${JSON.stringify(buildCatalog(), null, 2)}\n`;
