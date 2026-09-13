import path from 'node:path';
import { pathToFileURL } from 'node:url';
import type { Client } from '@modelcontextprotocol/client';
import type { PiContext } from '../../types.js';
import { publishMcpRuntimeState, runtimeStoreFor } from '../runtime-renderer.js';
import { isPlainRecord } from './config.js';
import { stringify } from './sanitize.js';
import { compileMcpSchemaValidator } from './schema-validator.js';

function requestSummary(value: unknown): string {
  const max = 1_200;
  const text = JSON.stringify(value)?.replace(/\s+/g, " ") ?? String(value);
  return text.length <= max ? text : `${text.slice(0, max - 1)}…`;
}

export function registerMcpClientHandlers(
  client: Client,
  serverName: string,
  ctx: PiContext | undefined,
  onToolsChanged: () => void,
): void {
  client.setRequestHandler("roots/list", async () => {
    const trusted = ctx?.isProjectTrusted
      ? Boolean(await ctx.isProjectTrusted())
      : false;
    if (!trusted || !ctx?.cwd) return { roots: [] };
    return {
      roots: [
        {
          uri: pathToFileURL(path.resolve(ctx.cwd)).href,
          name: path.basename(path.resolve(ctx.cwd)) || "workspace",
        },
      ],
    };
  });
  client.setRequestHandler("sampling/createMessage", async (request, { mcpReq: { signal } }) => {
    signal.throwIfAborted();
    const params = request.params as Record<string, unknown>;
    if (
      !ctx?.hasUI ||
      !ctx.ui?.confirm ||
      !ctx.model ||
      !ctx.modelRegistry?.complete
    ) {
      throw new Error(
        `MCP ${serverName} sampling denied: an interactive model session is required`,
      );
    }
    const approved = await ctx.ui.confirm(
      `Allow MCP sampling from ${serverName}?`,
      `${requestSummary(params["messages"])}\nmaxTokens: ${String(params["maxTokens"] ?? "server default")}`,
      { signal },
    );
    if (!approved) throw new Error(`MCP ${serverName} sampling denied by user`);
    signal.throwIfAborted();
    const maxTokens = params['maxTokens'];
    if (typeof maxTokens !== 'number' || !Number.isFinite(maxTokens) || maxTokens < 1)
      throw new Error(`MCP ${serverName} sampling requires a positive token limit`);
    const response = await ctx.modelRegistry.complete(
      ctx.model,
      {
        systemPrompt:
          typeof params["systemPrompt"] === "string"
            ? params["systemPrompt"]
            : undefined,
        messages: [
          {
            role: "user",
            content: stringify(params["messages"]),
            timestamp: Date.now(),
          },
        ],
      },
      {
        signal,
        maxTokens: Math.floor(maxTokens),
        ...(typeof params['temperature'] === 'number' ? { temperature: params['temperature'] } : {}),
      },
    );
    signal.throwIfAborted();
    const stopReason = isPlainRecord(response) ? response['stopReason'] : undefined;
    if (typeof stopReason === 'string' && ['error', 'aborted', 'toolUse', 'pending', 'deferred'].includes(stopReason))
      throw new Error(`MCP ${serverName} sampling did not complete: ${stopReason}`);
    const textParts = isPlainRecord(response) && Array.isArray(response['content'])
      ? response['content'].filter(isPlainRecord).filter(part => part['type'] === 'text' && typeof part['text'] === 'string')
      : [];
    if (textParts.length === 0) throw new Error(`MCP ${serverName} sampling returned no text`);
    const text = textParts.map(part => part['text']).join('\n');
    return {
      role: "assistant" as const,
      content: { type: "text" as const, text },
      model: ctx.model.id ?? "octocode-active-model",
      ...(typeof stopReason === 'string' ? {
        stopReason: stopReason === 'length' ? 'maxTokens' : stopReason === 'stop' ? 'endTurn' : stopReason,
      } : {}),
    };
  });
  client.setRequestHandler("elicitation/create", async (request, { mcpReq: { signal } }) => {
    signal.throwIfAborted();
    const params = request.params as Record<string, unknown>;
    if (!ctx?.hasUI || !ctx.ui?.confirm) return { action: "decline" as const };
    const message =
      typeof params["message"] === "string"
        ? params["message"]
        : `MCP ${serverName} requests input.`;
    const approved = await ctx.ui.confirm(
      `MCP input request from ${serverName}`,
      params['mode'] === 'url'
        ? `${message}\n\nURL: ${String(params['url'] ?? '')}`
        : `${message}\n\nRequested input schema:\n${stringify(params['requestedSchema'])}`,
      { signal },
    );
    if (!approved) return { action: "decline" as const };
    signal.throwIfAborted();
    if (params["mode"] === "url") {
      const url = typeof params["url"] === "string" ? params["url"] : undefined;
      if (url)
        ctx.ui.notify?.(
          `Open this approved MCP URL to continue: ${url}`,
          "info",
        );
      return { action: "accept" as const };
    }
    if (!ctx.ui.editor) return { action: "decline" as const };
    const value = await ctx.ui.editor(`Input for ${serverName}`, "{}");
    signal.throwIfAborted();
    if (value === undefined) return { action: "cancel" as const };
    let content: Record<string, string | number | boolean | string[]>;
    try {
      const parsed = JSON.parse(value) as unknown;
      if (!isPlainRecord(parsed))
        throw new Error("input must be a JSON object");
      const validation = compileMcpSchemaValidator(params['requestedSchema']).validate(parsed);
      if (!validation.valid)
        throw new Error(validation.errors.map(error => `${error.instancePath || '/'}: ${error.message}`).join('; '));
      content = {};
      for (const [key, raw] of Object.entries(parsed)) {
        if (
          typeof raw === "string" ||
          typeof raw === "number" ||
          typeof raw === "boolean"
        )
          content[key] = raw;
        else if (
          Array.isArray(raw) &&
          raw.every((item) => typeof item === "string")
        )
          content[key] = raw;
        else
          throw new Error(
            `${key} must be a string, number, boolean, or string array`,
          );
      }
    } catch (error) {
      ctx.ui.notify?.(
        `MCP input rejected: ${(error as Error).message}`,
        "warning",
      );
      return { action: "cancel" as const };
    }
    return { action: "accept" as const, content };
  });
  client.setNotificationHandler(
    "notifications/message",
    async (notification) => {
      const params = notification.params as Record<string, unknown>;
      const level =
        params["level"] === "error"
          ? "error"
          : params["level"] === "warning"
            ? "warning"
            : "info";
      runtimeStoreFor(ctx)
        ?.getState()
        .announce(
          `MCP ${serverName}: ${requestSummary(params["data"])}`,
          level,
        );
    },
  );
  client.setNotificationHandler(
    "notifications/progress",
    async (notification) => {
      const params = notification.params as Record<string, unknown>;
      publishMcpRuntimeState(ctx, {
        message: `progress ${String(params["progress"] ?? "")}${params["total"] !== undefined ? `/${String(params["total"])}` : ""}`,
      });
    },
  );
  client.setNotificationHandler('notifications/tools/list_changed', onToolsChanged);
}
