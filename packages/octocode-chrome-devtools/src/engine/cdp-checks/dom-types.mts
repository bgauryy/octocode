// Types shared by the DOM operation checks and the accessibility-tree ref collection (ax-snapshot).
// Type-only module: ax-snapshot is staged standalone, so it may import nothing at runtime.
import type { Payload } from '../cdp-connection.mjs';

/** One DOM step as a caller or DOM_STEPS gives it, before normalization. */
export type RawStep = {
  ref?: string;
  role?: string;
  name?: string;
  selector?: string;
  action?: string;
  value?: string | number | boolean | null;
  key?: string;
  toRef?: string;
  toSelector?: string;
};

/** Run options; each one wins over its DOM_* environment variable. */
export type DomOptions = {
  input?: string;
  stabilityMs?: number | string;
  settleMs?: number | string;
  waitMs?: number | string;
  waitText?: string;
  dialog?: string;
  diff?: boolean;
  traceEvents?: boolean;
  literalWaitText?: boolean;
  steps?: RawStep[];
  artifactName?: string;
};

/** Run-wide settings resolved from the options and the environment. */
export type DomConfig = {
  mode: 'js' | 'trusted';
  stabilityMs: number;
  settleMs: number;
  waitMs: number;
  waitText: string;
  acceptDialog: boolean;
  diff: boolean;
  traceEvents: boolean;
  literalWaitText: boolean | undefined;
};

/** A validated step; every text field is a string (empty when unset). */
export type DomStep = {
  ref: string;
  role: string;
  name: string;
  waitTarget: boolean;
  selector: string;
  action: string;
  value: string;
  key: string;
  toRef: string;
  toSelector: string;
  index: number;
};

export type DomContext = { cfg: DomConfig; log: (line: string) => void };

/**
 * The element check the page returns by value (found, visible, operation, bbox, before, ...),
 * then extended here with the action's effects and verification.
 */
export type StepDetails = Payload;

export type LocatedStep = {
  label: string;
  details: StepDetails;
  objectId: string | null;
};

/** What an action caused outside its element. */
export type DomEffects = {
  navigatedTo: string | null;
  dialog: { type: string; message: string; accepted: boolean } | null;
  popup: string | null;
};

/** A ref recorded in page-snapshot.json. */
export type SnapshotRef = {
  backendDOMNodeId: number;
  role?: string;
  name?: string;
  frame?: string;
};

/** The Accessibility.AXNode fields read here; child-frame nodes also carry their frame URL. */
export type AxNode = {
  nodeId: string;
  ignored?: boolean;
  ignoredReasons?: { name: string }[];
  role?: { value?: string };
  name?: { value?: string };
  properties?: { name: string; value?: { value?: unknown } }[];
  childIds?: string[];
  backendDOMNodeId?: number;
  frameUrl?: string;
};
export type FrameTree = {
  frame: { id: string; url?: string };
  childFrames?: FrameTree[];
};
export type SnapshotDocument = {
  nodes: {
    parentIndex: number[];
    backendNodeId: number[];
    nodeType: number[];
    attributes: number[][];
  };
  layout: { nodeIndex: number[]; styles: number[][]; bounds: number[][] };
};
export type Viewport = {
  pageX: number;
  pageY: number;
  clientWidth: number;
  clientHeight: number;
};
/** One kept node: a control, heading, named image or non-semantic clickable. */
export type RefUnit = {
  role: string;
  name: string;
  fullName: string;
  backendDOMNodeId: number;
  interactive: boolean;
  level?: number;
  frame?: string;
};
export type RefRegion = {
  role: string;
  name: string;
  backendDOMNodeId: number;
  start: number;
  end: number;
};

/** collectRefs options; viewport keeps only nodes inside it. */
export type CollectOptions = {
  rootBackendId?: number | null;
  viewport?: Viewport | null;
  clickable?: boolean;
  depth?: number;
};
