import { z } from "zod/v3";

import {
  AgentIdentitySchema,
  AGENT_PROTOCOL_IDENTIFIER_MAX_CHARS,
  AgentProtocolIdentifierSchema,
  AgentProtocolTimestampSchema,
  RESEARCH_PROTOCOL_VERSION,
  type AgentIdentity,
} from "./identity.js";
import { AgentExtensionsSchema } from "./extensions.js";
import { isAgentResultEvidenceLocator } from "./result.js";

export const AGENT_MESSAGE_KINDS = [
  "instruction",
  "question",
  "challenge",
  "evidence",
  "gap",
  "control",
] as const;
export const AgentMessageKindSchema = z.enum(AGENT_MESSAGE_KINDS);
export type AgentMessageKind = z.infer<typeof AgentMessageKindSchema>;
export const AGENT_WIRE_MESSAGE_KINDS = AGENT_MESSAGE_KINDS.map(
  (kind) => kind.toUpperCase() as Uppercase<AgentMessageKind>,
) as [Uppercase<AgentMessageKind>, ...Uppercase<AgentMessageKind>[]];

export const AGENT_SYNC_SENDERS = ["ORCHESTRATOR", "REMOTE_RESEARCH"] as const;
export const AgentSyncSenderSchema = z.enum(AGENT_SYNC_SENDERS);
export type AgentSyncSender = z.infer<typeof AgentSyncSenderSchema>;

export const AGENT_ACK_DELIVERY_MODES = ["turn_boundary", "restart", "remote"] as const;
export const AgentAckDeliveryModeSchema = z.enum(AGENT_ACK_DELIVERY_MODES);
export type AgentAckDeliveryMode = z.infer<typeof AgentAckDeliveryModeSchema>;

export const AGENT_MESSAGE_LIMITS = Object.freeze({
  contentChars: 4_000,
  evidenceRefs: 12,
  evidenceRefChars: 2_048,
  identifierChars: AGENT_PROTOCOL_IDENTIFIER_MAX_CHARS,
  inboxMessages: 32,
  attachments: 8,
  attachmentRefChars: 2_048,
  attachmentDescriptionChars: 500,
});

/**
 * Bounded, not locator-shaped: this is a free-form collaboration channel
 * (send_message passes model input straight through), unlike the terminal
 * AgentResult evidence/anchor refs — do not import that stricter rule here.
 */
const EvidenceRefsSchema = z.array(
  z.string().trim().min(1).max(AGENT_MESSAGE_LIMITS.evidenceRefChars),
).max(AGENT_MESSAGE_LIMITS.evidenceRefs);

export const AgentAttachmentKindSchema = z.string().trim().min(1).max(64).regex(
  /^[a-z][a-z0-9_]*$/,
  "attachment kind must be a lowercase snake_case identifier",
).describe("Open attachment kind (e.g. \"file\", \"image\", \"dataset\") — deployment-defined, not fixed here.");

/**
 * Deliberately pointer-shaped, never raw bytes — a `bytes`-carrying "thick"
 * version was prototyped and rejected: this package's own text bounds
 * (16k chars for AgentTaskContextSchema, "material facts") are already
 * smaller than one useful attachment, and AgentSyncMessageFrame is durable
 * and sequenced, so inlining bytes would turn the message log into a blob
 * store — persistence is explicitly runtime-owned (see README).
 *
 * `ref` alone would be pure relabeling of `evidenceRefs` (which already
 * accepts a plain URL/path verbatim) — the one thing an evidence pointer
 * can't do is commit to WHICH bytes it means. An evidenceRef points at
 * something pre-existing the receiver can independently re-read; a produced
 * artifact is mutable at its ref, so `digest` is required, not optional —
 * without it this degenerates into evidenceRefs with extra keys.
 */
export const AgentAttachmentSchema = z.object({
  kind: AgentAttachmentKindSchema,
  ref: z.string().trim().min(1).max(AGENT_MESSAGE_LIMITS.attachmentRefChars)
    .refine(isAgentResultEvidenceLocator, "attachment.ref must be a compact locator, not prose")
    .describe("Where the artifact actually lives (a URL, object key, or path) — never its bytes."),
  digest: z.string().trim().regex(
    /^[a-z0-9-]+:[A-Za-z0-9+/=_-]+$/,
    "digest must be \"algorithm:value\" (e.g. \"sha256:...\")",
  ).describe("Content commitment for the bytes at ref, since ref alone can't prove which version this is."),
  mimeType: z.string().trim().regex(/^[a-z0-9.+-]+\/[a-z0-9.+-]+$/, "mimeType must be \"type/subtype\"").optional(),
  description: z.string().trim().min(1).max(AGENT_MESSAGE_LIMITS.attachmentDescriptionChars).optional().describe(
    "Detail a receiver can't infer from kind/mimeType alone.",
  ),
}).strict();
export type AgentAttachment = z.infer<typeof AgentAttachmentSchema>;

export const AgentMessageInputSchema = z.object({
  kind: AgentMessageKindSchema.default("instruction").describe(
    "Message intent: instruction, question, challenge, evidence, gap, or control.",
  ),
  content: z.string().trim().min(1).max(AGENT_MESSAGE_LIMITS.contentChars).describe(
    "Material message for the recipient; omit routine progress and transport details.",
  ),
  evidenceRefs: EvidenceRefsSchema.default([]).describe(
    "Optional exact evidence locators that support this message.",
  ),
  replyTo: AgentProtocolIdentifierSchema.optional().describe(
    "Exact messageId being answered.",
  ),
  blocking: z.boolean().optional().describe(
    "True only if the sender is stalled awaiting this reply — omit/false for routine FYI traffic.",
  ),
  attachments: z.array(AgentAttachmentSchema).max(AGENT_MESSAGE_LIMITS.attachments).optional().describe(
    "Artifacts this message is about (a generated file, a screenshot, a dataset) — pointers with a "
    + "content digest, never raw bytes.",
  ),
  extensions: AgentExtensionsSchema.optional(),
}).strict();
export type AgentMessageInput = z.infer<typeof AgentMessageInputSchema>;

/** Durable Agent Sync transport frame shared by the server and both polling clients. */
export const AgentSyncMessageFrameSchema = z.object({
  schemaVersion: z.literal(1).describe("Wire schema generation for this frame shape; bump only on a breaking frame change."),
  sessionId: AgentProtocolIdentifierSchema.describe("The Agent Sync session this frame belongs to."),
  messageId: AgentProtocolIdentifierSchema,
  sequence: z.number().int().positive().describe("1-based, strictly increasing per session; used to detect gaps and reordering."),
  sender: AgentSyncSenderSchema.describe("Which side of the Agent Sync session sent this frame."),
  kind: z.enum(AGENT_WIRE_MESSAGE_KINDS).describe("Upper-cased wire form of AgentMessageKindSchema; see AGENT_WIRE_MESSAGE_KINDS."),
  content: AgentMessageInputSchema.shape.content,
  evidenceRefs: EvidenceRefsSchema,
  replyTo: AgentMessageInputSchema.shape.replyTo,
  blocking: AgentMessageInputSchema.shape.blocking,
  attachments: AgentMessageInputSchema.shape.attachments,
  extensions: AgentMessageInputSchema.shape.extensions,
  sentAt: AgentProtocolTimestampSchema,
}).strict();
export type AgentSyncMessageFrame = z.infer<typeof AgentSyncMessageFrameSchema>;

export const AgentMessageSchema = z.object({
  protocolVersion: z.literal(RESEARCH_PROTOCOL_VERSION),
  taskId: AgentProtocolIdentifierSchema,
  contextId: AgentProtocolIdentifierSchema,
  messageId: AgentProtocolIdentifierSchema,
  sequence: z.number().int().positive().describe("1-based, strictly increasing per taskId; used to detect gaps and reordering."),
  from: AgentIdentitySchema,
  to: AgentIdentitySchema,
  kind: AgentMessageKindSchema,
  content: AgentMessageInputSchema.shape.content,
  evidenceRefs: EvidenceRefsSchema,
  replyTo: AgentMessageInputSchema.shape.replyTo,
  blocking: AgentMessageInputSchema.shape.blocking,
  attachments: AgentMessageInputSchema.shape.attachments,
  extensions: AgentMessageInputSchema.shape.extensions,
  sentAt: AgentProtocolTimestampSchema,
}).strict();
export type AgentMessage = z.infer<typeof AgentMessageSchema>;

/**
 * Adapts the bounded, transport-agnostic AgentMessageInput a caller actually
 * constructs into the fully-addressed, sequenced AgentMessage the wire needs —
 * every runtime independently hand-rolls this join, so the protocol ships it once.
 */
export function toAgentMessage(
  input: AgentMessageInput,
  envelope: {
    taskId: string;
    contextId: string;
    messageId: string;
    sequence: number;
    from: AgentIdentity;
    to: AgentIdentity;
    sentAt: string;
  },
): AgentMessage {
  return AgentMessageSchema.parse({
    protocolVersion: RESEARCH_PROTOCOL_VERSION,
    ...envelope,
    kind: input.kind,
    content: input.content,
    evidenceRefs: input.evidenceRefs,
    replyTo: input.replyTo,
    blocking: input.blocking,
    attachments: input.attachments,
    extensions: input.extensions,
  });
}

export const AgentDeliveryAckSchema = z.object({
  protocolVersion: z.literal(RESEARCH_PROTOCOL_VERSION),
  accepted: z.boolean().describe("Whether the message was durably received by the recipient's inbox."),
  delivery: AgentAckDeliveryModeSchema.describe(
    "How delivery was confirmed: at the current turn boundary, after a restart, or by a remote peer.",
  ),
  taskId: AgentProtocolIdentifierSchema,
  messageId: AgentProtocolIdentifierSchema.optional(),
  acknowledgedBy: AgentIdentitySchema.optional().describe(
    "Who produced this ack, if tracked — sharpest under delivery=\"restart\", where identity may have changed.",
  ),
  acknowledgedAt: AgentProtocolTimestampSchema,
  reason: z.string().min(1).max(1_000).optional().describe("Required detail when accepted is false."),
  extensions: AgentExtensionsSchema.optional(),
}).strict().superRefine((ack, ctx) => {
  if (!ack.accepted && ack.reason == null) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["reason"],
      message: "Rejected delivery acknowledgements require a reason",
    });
  }
});
export type AgentDeliveryAck = z.infer<typeof AgentDeliveryAckSchema>;
