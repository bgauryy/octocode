import { z } from 'zod';

const MAX_COMMAND_LENGTH = 512;
const MAX_ARG_LENGTH = 1024;
const MAX_ARGS_COUNT = 64;
const EXTENSION_KEY_PATTERN = /^\.[a-z0-9._+-]+$/i;

const UserLanguageServerConfigSchema = z.looseObject({
  command: z
    .string()
    .min(1)
    .max(MAX_COMMAND_LENGTH)
    .refine(value => !/[\0\r\n]/.test(value), {
      message: 'command contains invalid control characters',
    }),
  args: z
    .array(
      z
        .string()
        .max(MAX_ARG_LENGTH)
        .refine(value => !/[\0\r\n]/.test(value), {
          message: 'argument contains invalid control characters',
        })
    )
    .max(MAX_ARGS_COUNT)
    .optional(),
  languageId: z.string().min(1).max(64),
  initializationOptions: z.record(z.string(), z.unknown()).optional(),
});

export const LSPConfigFileSchema = z.looseObject({
  languageServers: z
    .record(
      z.string().regex(EXTENSION_KEY_PATTERN),
      UserLanguageServerConfigSchema
    )
    .optional(),
});
