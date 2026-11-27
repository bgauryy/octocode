// Shapes of a validated browser plan and of the state its steps share while it runs.
import type { Handler, Payload } from '../cdp-connection.mjs';
import type { CheckSession } from '../cdp-runner-session.mjs';

/** A value taken from an earlier CDP result ($step) or listener event ($event). */
export type ResultReference = {
  $step?: number;
  $event?: string;
  pointer?: string;
};
export type FrameSelector = { id?: string; url?: string; selector?: string };
export type PageCondition = { text?: string; selector?: string; url?: string };

export type PlanStep = {
  op: string;
  url?: string;
  direction?: string;
  ignoreCache?: boolean;
  action?: string;
  ref?: string;
  selector?: string;
  role?: string;
  name?: string;
  value?: string | number | boolean;
  key?: string;
  toRef?: string;
  toSelector?: string;
  after?: PageCondition;
  timeoutMs?: number;
  optional?: boolean;
  frame?: FrameSelector;
  session?: string | ResultReference;
  fields?: string[];
  allowEmpty?: boolean;
  /** evaluate, cdp, extract: file name under saved/ that receives the step result. */
  save?: string;
  expression?: string;
  awaitPromise?: boolean;
  method?: string;
  acknowledge?: boolean;
  params?: Record<string, unknown>;
  domain?: string;
  member?: string;
  id?: string;
  event?: string;
  where?: Record<string, unknown>;
  listener?: string;
  count?: number;
  handle?: string | ResultReference;
  size?: number;
  close?: boolean;
  /** snapshot and screenshot: the standalone tool's recipe options. */
  options?: Record<string, unknown>;
  /** cdp: domains to enable first, and where the method runs. */
  enable?: string[];
  scope?: 'auto' | 'page' | 'browser';
};

export type Plan = {
  steps: PlanStep[];
  waitMs?: number;
  settleMs?: number;
  traceEvents?: boolean;
  /** act steps: accept or dismiss JavaScript dialogs (default dismiss). */
  dialog?: 'accept' | 'dismiss';
  /** CDP domains to enable first. */
  enable?: string[];
  observe?: { network?: boolean; afterMs?: number };
  commandMs?: number;
};

/** What waitCondition reports once the page matches. */
export type ConditionResult = {
  ready: boolean;
  url: string;
  matched: boolean;
  elapsedMs: number;
};
export type StepCondition = Partial<ConditionResult> & {
  text?: string;
  verified?: boolean;
};

/** Where a step runs: the attached target, a flattened session, or a child frame. */
export type PlanScope = CheckSession & { dispose?(): void };
/** A session or frame scope the run opened and must release. */
export type DisposableScope = CheckSession & { dispose(): void };

export type StepRow = {
  index: number;
  op: string;
  status: string;
  /** True on a step the plan marked optional; a failure of it did not stop the plan. */
  optional?: boolean;
  startedAt: string;
  finishedAt?: string;
  targetId?: string;
  sessionId?: string;
  source?: Record<string, unknown>;
  artifact?: string;
  /** The file a `save` step wrote under saved/. */
  saved?: string;
  error?: string;
  condition?: StepCondition;
  count?: number;
  note?: string;
  /** A cdp step that ran on the browser connection: `browser` (a page-scoped run leaves it unset). */
  scope?: 'browser';
  /** Domains enabled for this step (explicit, automatic, or a listener's), in the order sent. */
  enabled?: string[];
  chunks?: number;
  complete?: boolean;
  cleanupError?: string;
  observed?: number;
  matched?: number;
  /** What an evaluate step read: { result } is the page value. */
  data?: { result: unknown };
};

export type EventStream = {
  target: PlanScope;
  file: string;
  fd: number;
  observed: number;
  matched: number;
  last: unknown;
  event: string;
  handler: Handler;
};

export type PlanState = {
  plan: Plan;
  connection: CheckSession;
  cdp: CheckSession;
  streams: Map<string, EventStream>;
  /** CDP result artifacts by 1-based step number; references look them up by their raw $step. */
  artifacts: Map<unknown, string>;
  protocol: Payload | null;
  resolveParams: (value: unknown) => unknown;
  log: (line: string) => void;
};

/** The step being run, with its resolved timeout, journal row and scope. */
export type StepContext = {
  step: PlanStep;
  index: number;
  timeout: number;
  row: StepRow;
  target: PlanScope;
};

export type PlanOp = (
  state: PlanState,
  current: StepContext
) => Promise<void> | void;

/** Child-frame scopes attached during a run, by iframe target id. */
export type FrameScopes = Map<
  string,
  { scope: DisposableScope; sessionId: string }
>;
