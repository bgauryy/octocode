// The viewer's model: the store `S` every module reads (live.ts owns and folds it), the events the server and a
// saved log send, and the ops the viewer sends back. Payloads follow their producers in src/runtime (pipeline.ts,
// serve.ts, serve-answers.ts, lsp.ts, verify.ts); only the fields the viewer reads are typed here.
import type { Row } from '../runtime/query-ops.ts';
import type {
  CallEdge,
  CallStats,
  Chunk,
  Community,
  Confidence,
  Coverage,
  Declaration,
  FileCheck,
  FileMetrics,
  GraphMetrics,
  ImportEdge,
  RefreshedPayload,
  RunEvent,
  Verification,
} from '../runtime/types.ts';

/** Who backs an edge: the static graph, the language server, both, or the stitch between chunks. */
export type Via = 'graph' | 'lsp' | 'both' | 'stitched';
export type EdgeKind = 'imports' | 'calls';
export type FeedLevel = 'info' | 'ok' | 'warn' | 'bad';

/** A store node: a listed file, a parsed file or a workspace package. */
export interface ViewNode {
  id: string;
  path: string;
  group: string;
  label: string;
  inGraph: boolean;
  kind: 'file' | 'package';
  fanIn: number;
  fanOut: number;
  born: number;
  roles: string[];
  language?: string;
  chunk?: string;
  manifest?: string;
  /** The planned chunk whose path holds this file. */
  planChunk?: string;
  metrics?: FileMetrics;
  check?: FileCheck;
}

/** A file-level store edge; calls fold their symbol pairs into it. Endpoints are node ids. */
export interface ViewEdge {
  key: string;
  kind: EdgeKind;
  source: string;
  target: string;
  lines: Set<number>;
  count: number;
  confidence: Confidence;
  via: Via;
  test: boolean;
  born: number;
  crossChunk: boolean;
  fromStitch: boolean;
  /** A production import: it counts toward its endpoints' fan-in and fan-out. */
  counted: boolean;
  /** The file pair's weight from the latest metrics event. */
  w?: number;
}

/** A symbol-level call between `file#name` ids. */
export interface ViewCall {
  source: string;
  target: string;
  lines: Set<number>;
  via: Via;
  confidence?: Confidence;
  crossChunk: boolean;
  fromStitch: boolean;
  file: string;
}

/** What a witness row, a stitched edge or an LSP-confirmed call adds to a store edge. */
export interface EdgeWitness {
  confidence?: Confidence;
  via?: Via;
  crossChunk?: boolean;
  fromStitch?: boolean;
  test?: boolean;
  count?: number;
  lines?: number[];
  line?: number | null;
}

export interface CallWitness extends EdgeWitness {
  source: string;
  target: string;
}

export interface FileRow {
  id: string;
  language?: string;
  roles: string[];
}

export interface WitnessRow {
  edge: string;
  from: string;
  to: string;
  file?: string;
  line?: number;
  confidence: Confidence;
  detail?: string;
  test: boolean;
}

export type RowsPayload = { chunk: string; page?: number; total?: number } & (
  | { kind: 'files'; rows: FileRow[] }
  | { kind: 'symbols'; rows: Declaration[] }
  | { kind: 'witnesses'; rows: WitnessRow[] }
);

export type ChunkStatus =
  'queued' | 'ingesting' | 'paging' | 'done' | 'error' | 'split';
export type PlannedChunk = Pick<
  Chunk,
  'id' | 'path' | 'files' | 'loose' | 'package'
>;

/** A chunk's state change; a split names the chunks that replace it. */
export type ChunkPayload = {
  id: string;
  error?: string;
  ms?: number;
  counts?: Coverage;
  snapshot?: unknown;
  build?: string;
} & (
  | { state: 'split'; into: PlannedChunk[] }
  | { state: Exclude<ChunkStatus, 'split'>; into?: undefined }
);

export interface ChunkState extends PlannedChunk {
  state: ChunkStatus;
  error?: string;
  /** Run time when ingesting started. */
  startT?: number;
  ms?: number;
  counts?: Coverage;
  snapshot?: unknown;
  build?: string;
  /** Rows received per kind. */
  rows?: Record<string, number>;
}

export interface PhasePayload {
  id: string;
  state: string;
  ms?: number;
  note?: string;
  lspState?: string;
}

export interface PhaseState {
  id: string;
  start?: number;
  state: string;
  ms: number | null;
  note?: string;
  info: PhasePayload;
}

export interface ProgressPayload {
  phase: string;
  done: number;
  total: number;
}

export interface StitchedPayload {
  imports: ImportEdge[];
  calls: CallEdge[];
  packages: { id: string; label: string; chunk?: string; manifest?: string }[];
  unresolved: unknown[];
  uncovered: string[];
}

export interface LspPayload {
  update: { source: string; target: string; via: Via }[];
  added: CallWitness[];
  stats: CallStats;
}

export interface VerifyPayload {
  rows: (FileCheck & { id: string })[];
  summary: Verification;
}

/** Per-file metrics as columns aligned to `ids`; weighted file edges as index columns (pipeline.ts metricsEvent). */
export interface MetricsPayload extends Pick<
  GraphMetrics,
  'weightRule' | 'stats' | 'timings' | 'cycles' | 'packages'
> {
  ids: string[];
  columns: { [K in keyof FileMetrics]: FileMetrics[K][] };
  edges: {
    source: number[];
    target: number[];
    weight: number[];
    test: number[];
  };
  communities: Community[];
  callStats: CallStats;
  verification?: Verification;
}

export interface ToolCall {
  id: string;
  state: 'start' | 'done' | 'error';
  tool: string;
  detail?: string;
  page?: string;
  ms?: number;
  bytes?: number;
  rows?: number;
  total?: number;
  more?: boolean;
  error?: string;
}

export interface ToolSummary {
  tool: string;
  calls: number;
  pages: number;
  rows: number;
  ms: number;
  max: number;
  bytes: number;
  errors: number;
  running: number;
}

export interface NotePayload {
  text: string;
  level?: FeedLevel;
}

/** An astSearch outline entry: a declaration with its members, or a `line\ttext` row. */
export type AstItem = string | AstSymbol;
export interface AstSymbol {
  line: number;
  symbolName: string;
  kind: string;
  endLine?: number;
  members?: AstItem[];
}
export interface AstResult {
  state: 'start' | 'done';
  file?: string;
  symbols?: AstItem[];
  error?: string;
}

export type LspOperation = 'callers' | 'callees' | 'references';
export interface LspRequest {
  file: string;
  symbolName: string;
  line?: number;
  operation: LspOperation;
}
export interface LspRow {
  path: string;
  line?: number;
  symbolName?: string;
}
export type LspResult = { req: LspRequest } & (
  | { state: 'start' }
  | { state: 'done'; rows: LspRow[]; notes?: string[]; error?: string }
);

export interface RunInfo {
  root: string;
  name: string;
}

export interface DonePayload {
  ms: number;
  data: string;
  brief: string;
}

export interface ErrorPayload {
  message: string;
}

export type ChatRole = 'user' | 'agent' | 'system';
export type ChatState = 'queued' | 'delivered' | 'replied' | 'failed';
/** The selected node attached to a chat message. */
export interface ChatContext {
  file: string;
  line: number;
  metrics?: FileMetrics;
}
/** A chat message, or a later delivery-state update of the message with the same id. */
export interface ChatMessage {
  id: string;
  role?: ChatRole;
  text?: string;
  context?: { file?: string; line?: number };
  state?: ChatState;
  error?: string;
  at?: number;
  nodes?: string[];
}

/** A graph answer: the server's (or the saved-graph worker's) result, or the bar's pending/offline placeholder. */
export interface QueryAnswer {
  text: string;
  ok?: boolean;
  op?: string;
  pending?: boolean;
  from?: string;
  summary?: string;
  error?: string;
  scope?: { isPartial: boolean; partialReasons: string[] };
  revision?: string;
  lspStatus?: string;
  nodes?: string[];
  rows?: Row[];
  steps?: unknown[];
  crossings?: unknown[];
  stats?: { objective?: string };
  candidates?: { query: string; id: string }[];
  links?: { community: number; label?: string; weight: number }[];
}

export interface WikiPageRow {
  slug: string;
  label?: string;
  community?: number | null;
  size?: number;
  notes?: string;
}
export interface WikiPayload {
  pages?: WikiPageRow[];
  updated?: string[];
  created?: string[];
  removed?: string[];
}
/** The answer to a `wiki` op: the page, or no markdown and why. */
export type WikiPagePayload =
  | { slug: string; markdown: string; file?: undefined; error?: undefined }
  | {
      slug: string | null;
      file?: string | null;
      markdown: null;
      error?: string;
    };

/** Every event type the viewer handles, with its payload. */
export interface EventMap {
  run: RunInfo;
  phase: PhasePayload;
  skeleton: { files: string[]; total?: number };
  plan: { chunks: PlannedChunk[] };
  chunk: ChunkPayload;
  rows: RowsPayload;
  progress: ProgressPayload;
  stitched: StitchedPayload;
  'graph-ready': MetricsPayload;
  lsp: LspPayload;
  verify: VerifyPayload;
  metrics: MetricsPayload;
  tool: ToolCall;
  note: NotePayload;
  ast: AstResult & { file: string };
  lspResult: LspResult;
  done: DonePayload;
  error: ErrorPayload;
  'chunk-reset': { id: string; files: string[] };
  refreshed: RefreshedPayload;
  chat: ChatMessage;
  query: QueryAnswer;
  wiki: WikiPayload;
  wikiPage: WikiPagePayload;
}
export type EventType = keyof EventMap;
/** A handled event as the log stores it: its payload plus the sequence number and run time the log adds. */
export type LiveEvent<K extends EventType = EventType> = {
  [P in K]: { type: P; seq: number; t: number } & EventMap[P];
}[K];
/** An event as it arrives over the socket or from a saved log; its type may be one this viewer does not handle. */
export type LoggedEvent = RunEvent & { seq: number };

/** Requests the viewer sends over the socket (serve.ts handles each op). */
export type Op =
  | { op: 'hello'; since: number }
  | { op: 'cancel' }
  | { op: 'ast'; file: string }
  | ({ op: 'lsp' } & LspRequest)
  | { op: 'chat'; text: string; context?: ChatContext }
  | { op: 'query'; text: string }
  | { op: 'wiki'; slug?: string; file?: string };

export type SelectFn = (
  id: string | null,
  options?: { focus?: boolean }
) => void;
export type HighlightFn = (ids?: string[]) => void;
export type SendFn = (op: Op) => void;
/** Requests answered over the socket; the answer lands in the store as an event. */
export interface AskApi {
  ast(file: string): void;
  lsp(req: LspRequest): void;
}
/** What the chat, query and wiki drawers get from the viewer. */
export interface ViewDeps {
  send: SendFn;
  select: SelectFn;
  highlight: HighlightFn;
}

/** The store: everything the event stream has said so far. */
export interface LiveState {
  offline: boolean;
  run: RunInfo | null;
  root: string;
  phases: Map<string, PhaseState>;
  chunks: Map<string, ChunkState>;
  progress: Map<string, ProgressPayload>;
  nodes: Map<string, ViewNode>;
  edges: Map<string, ViewEdge>;
  calls: Map<string, ViewCall>;
  symbols: Map<string, Declaration[]>;
  metrics: MetricsPayload | null;
  stitched: StitchedPayload | null;
  lspStats: CallStats | null;
  done: DonePayload | null;
  error: ErrorPayload | null;
  tools: ToolCall[];
  toolsById: Map<string, ToolCall>;
  toolSummary: Map<string, ToolSummary>;
  notes: NotePayload[];
  ast: Map<string, AstResult>;
  lspResults: Map<string, LspResult>;
  skeletonTotal: number;
  lastSeq: number;
  /** Latest run time seen (ms since the run started). */
  t: number;
  selected: string | null;
  highlight: Set<string>;
  groupSlot: Map<string, number>;
  verification: Verification | null;
  /** True while a replayed batch is applied: no feed lines, no camera moves. */
  replaying?: boolean;
  /** performance.now() when the last batch was applied, to run the clock between events. */
  clockAt?: number;
}

/** Document events the viewer modules use to follow each other without importing each other. */
export interface CommunityDetail {
  bucket: string;
  community: number | null;
}
declare global {
  interface DocumentEventMap {
    'gv:select': CustomEvent<string | null>;
    'gv:community': CustomEvent<CommunityDetail>;
    'gv:graph': Event;
  }
}
