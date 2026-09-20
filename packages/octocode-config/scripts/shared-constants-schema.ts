import { z } from 'zod';

const envKeySchema = z
  .string()
  .regex(/^[A-Z][A-Z0-9_]*$/, 'must be an uppercase environment variable name');

const uniqueArray = (field: string) =>
  z.array(envKeySchema).min(1).superRefine((values, context) => {
    const seen = new Set<string>();
    for (const [index, value] of values.entries()) {
      if (seen.has(value)) {
        context.addIssue({
          code: 'custom',
          message: `${field} contains duplicate value ${value}`,
          path: [index],
        });
      }
      seen.add(value);
    }
  });

const lowercaseIdentifierArray = (field: string) =>
  z
    .array(z.string().regex(/^[a-z][a-z0-9]*$/))
    .min(1)
    .superRefine((values, context) => {
      if (new Set(values).size !== values.length) {
        context.addIssue({
          code: 'custom',
          message: `${field} must not contain duplicates`,
        });
      }
    });

const defaultValuesSchema = z.record(
  z.string().regex(/^[a-z][A-Za-z0-9]*$/),
  z.union([z.string(), z.number().int(), z.boolean()])
);

const validationBoundsSchema = z
  .record(
    z.string().regex(/^(min|max)[A-Z][A-Za-z0-9]*$/),
    z.number().int().nonnegative()
  )
  .superRefine((bounds, context) => {
    for (const [minName, min] of Object.entries(bounds)) {
      if (!minName.startsWith('min')) continue;
      const maxName = `max${minName.slice(3)}`;
      const max = bounds[maxName];
      if (max === undefined) {
        context.addIssue({
          code: 'custom',
          message: `${minName} requires matching ${maxName}`,
          path: [minName],
        });
      } else if (min > max) {
        context.addIssue({
          code: 'custom',
          message: `${minName} must not exceed ${maxName}`,
          path: [minName],
        });
      }
    }

    for (const maxName of Object.keys(bounds)) {
      if (!maxName.startsWith('max')) continue;
      const minName = `min${maxName.slice(3)}`;
      if (bounds[minName] === undefined) {
        context.addIssue({
          code: 'custom',
          message: `${maxName} requires matching ${minName}`,
          path: [maxName],
        });
      }
    }
  });

export const sharedConstantsSchema = z
  .strictObject({
    _comment: z.string().optional(),
    configSchemaVersion: z.number().int().positive(),
    configFileName: z.string().min(1),
    runtimeSurfaces: lowercaseIdentifierArray('runtimeSurfaces'),
    runtimeSurfaceDefault: z.string(),
    outputFormats: lowercaseIdentifierArray('outputFormats'),
    storageModes: lowercaseIdentifierArray('storageModes'),
    defaultValues: defaultValuesSchema,
    envTokenVars: uniqueArray('envTokenVars'),
    protectedKeys: uniqueArray('protectedKeys'),
    configSourceEnvKeys: uniqueArray('configSourceEnvKeys'),
    validationBounds: validationBoundsSchema,
  })
  .superRefine((constants, context) => {
    if (!constants.runtimeSurfaces.includes(constants.runtimeSurfaceDefault)) {
      context.addIssue({
        code: 'custom',
        message: 'runtimeSurfaceDefault must be a member of runtimeSurfaces',
        path: ['runtimeSurfaceDefault'],
      });
    }

    const outputFormat = constants.defaultValues['outputFormat'];
    if (
      typeof outputFormat !== 'string' ||
      !constants.outputFormats.includes(outputFormat)
    ) {
      context.addIssue({
        code: 'custom',
        message: 'defaultValues.outputFormat must be a member of outputFormats',
        path: ['defaultValues', 'outputFormat'],
      });
    }

    const storageMode = constants.defaultValues['storageMode'];
    if (
      typeof storageMode !== 'string' ||
      !constants.storageModes.includes(storageMode)
    ) {
      context.addIssue({
        code: 'custom',
        message: 'defaultValues.storageMode must be a member of storageModes',
        path: ['defaultValues', 'storageMode'],
      });
    }

    const protectedKeys = new Set(constants.protectedKeys);
    for (const [index, tokenVar] of constants.envTokenVars.entries()) {
      if (!protectedKeys.has(tokenVar)) {
        context.addIssue({
          code: 'custom',
          message: `${tokenVar} must also be protected`,
          path: ['envTokenVars', index],
        });
      }
    }
  });

export type SharedConstants = z.infer<typeof sharedConstantsSchema>;
