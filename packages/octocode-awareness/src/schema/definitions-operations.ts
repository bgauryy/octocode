/* v8 ignore file -- exercised through built CLI and isolated-package subprocess tests */
import { z } from 'zod';
import { signalDataSchema } from '../signal-data.js';
import {
  agentId, nonEmptyText, artifactScope, importanceLevel,
  notificationKind, fileList, refIds,
} from './common.js';

export const operationSchemas = {
  agent_signal: z
    .object({
      action: z.enum(["publish", "list", "reply", "resolve", "ack"]).describe("Action."),
      agent_id: agentId.describe("Actor."),
      workspace_path: z.string().trim().min(1).max(1024).optional().describe("Workspace channel."),
      artifact: artifactScope.optional(),
      repo: z.string().trim().min(1).max(256).optional(),
      ref: z.string().trim().min(1).max(256).optional(),
      kind: notificationKind.optional().describe("Signal kind."),
      subject: nonEmptyText("Subject.", 200).optional(),
      body: nonEmptyText("Body.", 4000).optional(),
      data: z.union([z.string().max(4000), signalDataSchema]).optional().describe('Machine message: {type, payload}, or its JSON string. Body plus envelope is limited to 4000 bytes. Read with include_bodies; dispatch on data.type, not subject.'),
      to_agents: z.array(agentId).max(50).default([]).describe("Recipients."),
      files: fileList,
      refs: refIds,
      importance: importanceLevel.default(5),
      in_reply_to: z.string().trim().min(1).max(128).optional().describe("Reply target."),
      thread_id: z.string().trim().min(1).max(128).optional().describe("Thread id."),
      signal_id: z.array(z.string().trim().min(1).max(128)).max(200).default([]).describe("Signal ids."),
      unread_only: z.boolean().default(true),
      mark_read: z.boolean().default(false),
      kinds: z.array(notificationKind).max(8).default([]),
      limit: z.number().int().min(1).max(200).default(20),
      cursor: z.string().min(1).max(1024).optional().describe("Opaque signal-list continuation from the preceding page."),
      include_bodies: z
        .boolean()
        .default(false)
        .describe("Include full signal bodies on list (default summarizes to 160 chars)."),
      format: z
        .enum(["json", "hook"])
        .default("json")
        .describe("list only: json rows, or hook briefing shape for host notify delivery."),
    })
    .strict()
    .describe("Signal actions."),
};
