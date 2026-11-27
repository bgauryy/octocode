/** The record types a command writes, from the catalog's `recordTypes` entries. */
export interface RecordTypeEntry { readonly type: string; readonly writer: string; readonly dataSchema: unknown }

/**
 * The `data` variants of the `record` command input: one per record type whose writer is `record`, each pairing the type
 * with its payload schema. Catalog loading validates against these and type generation compiles them, so the two agree.
 */
export function recordVariants(recordTypes: readonly RecordTypeEntry[]): { properties: { type: { const: string }; data: unknown } }[] {
  return recordTypes.filter(item => item.writer === 'record').map(item => ({ properties: { type: { const: item.type }, data: structuredClone(item.dataSchema) } }));
}
