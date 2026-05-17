import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { z } from 'zod/v4';
import { logPromptCall } from '../session.js';
import { withOutputSanitization } from '../utils/secureServer.js';
import type { CompleteMetadata } from '@octocodeai/octocode-core';

type PromptHandler = (args: unknown) => Promise<{
  messages: Array<{
    role: 'user' | 'assistant';
    content: { type: 'text'; text: string };
  }>;
}>;

/**
 * Register all prompts with the MCP server
 * Iterates over the prompts defined in the metadata and registers them dynamically
 */
export function registerPrompts(
  server: McpServer,
  content: CompleteMetadata
): void {
  const prompts = content.prompts;

  if (!prompts) {
    return;
  }

  // Wrap with output-sanitization + crash-isolation layer so a throwing
  // prompt handler can never crash the MCP server and any secrets in error
  // messages are redacted before reaching the JSON-RPC error channel.
  const secureServer = withOutputSanitization(server);

  for (const prompt of Object.values(prompts)) {
    if (
      !prompt ||
      typeof prompt.name !== 'string' ||
      prompt.name.trim().length === 0 ||
      typeof prompt.description !== 'string' ||
      prompt.description.trim().length === 0 ||
      typeof prompt.content !== 'string' ||
      prompt.content.trim().length === 0
    ) {
      continue;
    }

    const argsShape: Record<string, z.ZodType<unknown>> = {};
    if (prompt.args && Array.isArray(prompt.args)) {
      for (const arg of prompt.args) {
        if (!arg || typeof arg.name !== 'string') {
          continue;
        }
        let schema: z.ZodType<unknown> = z.string().describe(arg.description);
        if (!arg.required) {
          schema = schema.optional();
        }
        argsShape[arg.name] = schema;
      }
    }

    const handler: PromptHandler = async incomingArgs => {
      await logPromptCall(prompt.name);

      let text = prompt.content;

      if (incomingArgs && Object.keys(incomingArgs).length > 0) {
        text += '\n\nUse Input\n\n';
        for (const [key, value] of Object.entries(incomingArgs)) {
          if (value !== undefined && value !== null) {
            text += `${key}: ${String(value)}\n`;
          }
        }
      }

      return {
        messages: [
          {
            role: 'user',
            content: {
              type: 'text',
              text,
            },
          },
        ],
      };
    };

    const promptOptions = {
      description: prompt.description,
      ...(Object.keys(argsShape).length > 0 ? { argsSchema: argsShape } : {}),
    };
    secureServer.registerPrompt(prompt.name, promptOptions, handler);
  }
}
