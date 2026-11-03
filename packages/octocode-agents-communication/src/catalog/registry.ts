/** Shared pieces of the catalog source: the schema registry, definition shapes and tool annotation presets. */
import { z } from 'zod';

/** Every schema that becomes a named entry (`$defs`, a command input or a result) carries its id here; `z.toJSONSchema` turns repeated use into `$ref`. */
export const registry = z.registry<{ id: string }>();

/** Ids registered by {@link define}, in definition order: the keys of the catalog's `$defs`. */
export const definitionIds: string[] = [];

/** A schema every use of which is a `$ref` to one `$defs` entry. */
export function define<T extends z.ZodType>(id: string, schema: T): T {
  registry.add(schema, { id });
  definitionIds.push(id);
  return schema;
}

/** A bare `$ref` to a defined schema, where the catalog entry is the reference itself rather than a copy. */
export const ref = (schema: z.ZodType): z.ZodType => z.lazy(() => schema);

/** MCP behavior hints; the catalog only ever sets these three. */
export interface Annotations { readonly readOnlyHint: boolean; readonly destructiveHint: boolean; readonly openWorldHint: boolean }
export const readOnly: Annotations = { readOnlyHint: true, destructiveHint: false, openWorldHint: false };
export const mutating: Annotations = { readOnlyHint: false, destructiveHint: false, openWorldHint: false };

export interface CommandDefinition {
  readonly name: string;
  readonly description: string;
  readonly input?: z.ZodType;
  readonly annotations?: Annotations;
  readonly usage: string;
  readonly cliFlags: readonly string[];
  /** Conditional rules over the CLI flags, in JSON Schema: not expressible as a Zod type. */
  readonly cliSchema?: Record<string, unknown>;
  /** What the command prints once null members are stripped. */
  readonly result: z.ZodType;
}

export interface ToolDefinition { readonly name: string; readonly annotations: Annotations }

export interface RecordTypeDefinition {
  readonly type: string;
  readonly writer: string;
  readonly description: string;
  readonly dataSchema: z.ZodType;
  readonly example: Record<string, unknown>;
  readonly routing: string;
  readonly fetch: Record<string, unknown>;
}
