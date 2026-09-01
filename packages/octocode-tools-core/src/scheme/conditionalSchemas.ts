import { z } from 'zod';

type AnyZodObject = z.ZodObject<z.ZodRawShape>;

const SELECTOR_FIELDS = [
  'fullContent',
  'matchString',
  'matchStringIsRegex',
  'matchStringCaseSensitive',
  'startLine',
  'endLine',
] as const;

const DIRECTORY_EXTRACTION_FIELDS = [
  ...SELECTOR_FIELDS,
  'contextLines',
  'charOffset',
  'charLength',
  'minify',
] as const;

function omitFields(
  shape: z.ZodRawShape,
  fields: readonly string[]
): z.ZodRawShape {
  const omitted = new Set(fields);
  return Object.fromEntries(
    Object.entries(shape).filter(([name]) => !omitted.has(name))
  );
}

function copyDescription<T extends z.ZodTypeAny>(
  source: z.ZodTypeAny,
  target: T
): T {
  return source.description
    ? (target.describe(source.description) as T)
    : target;
}

export function getSchemaField(
  shape: z.ZodRawShape,
  name: string
): z.ZodTypeAny {
  const field = shape[name] as z.ZodTypeAny | undefined;
  if (!field) throw new TypeError(`Expected schema field: ${name}`);
  return field;
}

export function getRequiredSchemaField(
  shape: z.ZodRawShape,
  name: string
): z.ZodTypeAny {
  return getSchemaField(shape, name).nonoptional();
}

function contentFileVariants(base: AnyZodObject, typeField?: z.ZodTypeAny) {
  const commonShape = omitFields(base.shape, [
    ...SELECTOR_FIELDS,
    ...(typeField ? ['type'] : []),
  ]);
  const withType = typeField ? { type: typeField } : {};
  const inactiveFullContent = copyDescription(
    getSchemaField(base.shape, 'fullContent'),
    z.literal(false).optional()
  );
  const fullContent = copyDescription(
    getSchemaField(base.shape, 'fullContent'),
    z.literal(true)
  );

  const defaultMode = z
    .object({
      ...commonShape,
      ...withType,
      fullContent: inactiveFullContent,
    })
    .strict();
  const fullMode = z
    .object({
      ...commonShape,
      ...withType,
      fullContent,
    })
    .strict();
  const matchMode = z
    .object({
      ...commonShape,
      ...withType,
      fullContent: inactiveFullContent,
      matchString: getRequiredSchemaField(base.shape, 'matchString'),
      matchStringIsRegex: getSchemaField(base.shape, 'matchStringIsRegex'),
      matchStringCaseSensitive: getSchemaField(
        base.shape,
        'matchStringCaseSensitive'
      ),
    })
    .strict();
  const rangeMode = z
    .object({
      ...commonShape,
      ...withType,
      fullContent: inactiveFullContent,
      startLine: getRequiredSchemaField(base.shape, 'startLine'),
      endLine: getRequiredSchemaField(base.shape, 'endLine'),
    })
    .strict()
    .superRefine((query, ctx) => {
      if (
        typeof query.startLine === 'number' &&
        typeof query.endLine === 'number' &&
        query.endLine < query.startLine
      ) {
        ctx.addIssue({
          code: 'custom',
          path: ['endLine'],
          message: 'Set endLine greater than or equal to startLine.',
        });
      }
    });

  return [defaultMode, fullMode, matchMode, rangeMode] as const;
}

/**
 * Keep the public bulk root an object while making selector modes visible to
 * JSON Schema consumers through a union at `queries.items`.
 */
export function createContentSelectorQuerySchema(
  base: AnyZodObject,
  options: { githubDirectoryMode?: boolean } = {}
): z.ZodTypeAny {
  if (!options.githubDirectoryMode) {
    return z.union(contentFileVariants(base));
  }

  const fileType = copyDescription(
    getSchemaField(base.shape, 'type'),
    z.literal('file').optional()
  );
  const directoryType = copyDescription(
    getSchemaField(base.shape, 'type'),
    z.literal('directory')
  );
  const directoryShape = omitFields(base.shape, [
    ...DIRECTORY_EXTRACTION_FIELDS,
    'type',
  ]);
  const directoryMode = z
    .object({ ...directoryShape, type: directoryType })
    .strict();

  return z.union([...contentFileVariants(base, fileType), directoryMode]);
}
