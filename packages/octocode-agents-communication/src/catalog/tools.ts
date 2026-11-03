/** The commands MCP exposes as tools, with their behavior hints. */
import { mutating, readOnly, type ToolDefinition } from './registry.ts';

export const tools: readonly ToolDefinition[] = [
  { name: "peers", annotations: readOnly },
  { name: "set_status", annotations: mutating },
  { name: "send_message", annotations: mutating },
  { name: "inbox", annotations: readOnly },
  { name: "complete", annotations: mutating },
  { name: "subscribe", annotations: mutating },
  { name: "lock", annotations: mutating },
  { name: "renew", annotations: mutating },
  { name: "unlock", annotations: mutating },
  { name: "lock_many", annotations: mutating },
  { name: "check_write", annotations: readOnly },
  { name: "share_document", annotations: mutating },
  { name: "read_document", annotations: readOnly },
  { name: "activity", annotations: readOnly },
  { name: "context", annotations: readOnly },
  { name: "locks", annotations: readOnly },
  { name: "fetch", annotations: readOnly },
  { name: "record", annotations: mutating },
];
