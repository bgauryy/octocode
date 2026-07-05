/**
 * Ambient module declarations for runtime dependencies that either lack
 * published type declarations or have incorrect package.json exports fields.
 *
 * typebox — Pi's runtime TypeBox provider; dynamically imported in tools.
 * (Not `@sinclair/typebox`; installed and resolved by Pi's own runtime.)
 */
declare module 'typebox' {
  type TSchema = Record<string, unknown>;

  interface SchemaOptions {
    description?: string;
    [key: string]: unknown;
  }

  interface IntegerOptions extends SchemaOptions {
    minimum?: number;
    maximum?: number;
  }

  interface ArrayOptions extends SchemaOptions {
    minItems?: number;
    maxItems?: number;
  }

  export const Type: {
    Object(props: Record<string, TSchema>, opts?: SchemaOptions): TSchema;
    String(opts?: SchemaOptions): TSchema;
    Optional(schema: TSchema): TSchema;
    Array(schema: TSchema, opts?: ArrayOptions): TSchema;
    Integer(opts?: IntegerOptions): TSchema;
    Boolean(opts?: SchemaOptions): TSchema;
    Literal<T extends string | number | boolean>(val: T): TSchema;
    Union(schemas: TSchema[], opts?: SchemaOptions): TSchema;
    Unsafe(schema: Record<string, unknown>): TSchema;
  };
}
