/**
 * Zod schemas for LSP configuration file validation.
 *
 * Validates parsed JSON from lsp-servers.json config files
 * before type-asserting to LSPConfigFile.
 */

import { z } from 'zod/v4';

const MAX_COMMAND_LENGTH = 512;
const MAX_ARG_LENGTH = 1024;
const MAX_ARGS_COUNT = 64;
const EXTENSION_KEY_PATTERN = /^\.[a-z0-9._+-]+$/i;

/**
 * Schema for user-defined language server configuration.
 */
const UserLanguageServerConfigSchema = z
  .object({
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
  })
  .passthrough();

/**
 * Schema for the LSP config file (lsp-servers.json).
 */
export const LSPConfigFileSchema = z
  .object({
    languageServers: z
      .record(
        z.string().regex(EXTENSION_KEY_PATTERN),
        UserLanguageServerConfigSchema
      )
      .optional(),
  })
  .passthrough();
