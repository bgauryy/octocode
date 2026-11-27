// The fields measure-query reads from the performance/network/storage measure artifacts and HAR rows.
// The artifacts are this package's own output; every field is optional because a run may omit it.
export type MeasureFinding = { code?: string; [field: string]: unknown };

/** A JSON scalar: the filters stringify text fields because an artifact may hold any scalar there. */
export type JsonScalar = string | number | boolean | null;

/** A network/resource/cookie row: only the fields the filters read are named. */
export type MeasureRow = {
  url?: JsonScalar;
  name?: JsonScalar;
  kind?: JsonScalar;
  type?: JsonScalar;
  ms?: number;
  duration?: number;
  initiatorType?: JsonScalar;
  domain?: JsonScalar;
  [field: string]: unknown;
};

export type PerfMeasure = {
  score?: {
    health?: number;
    findings?: MeasureFinding[];
    slowResources?: MeasureRow[];
  };
  longTasks?: unknown[];
  resources?: MeasureRow[];
  measures?: unknown[];
  paints?: unknown;
  fcp?: unknown;
  lcp?: unknown;
  cls?: unknown;
};

export type NetMeasure = {
  health?: number;
  findings?: MeasureFinding[];
  failures?: MeasureRow[];
  slow?: MeasureRow[];
  sample?: MeasureRow[];
  counts?: { failed?: number; slow?: number; requests?: number };
  byKind?: unknown;
  byStatus?: unknown;
};

export type StorageMeasure = {
  score?: { health?: number; findings?: MeasureFinding[] };
  cookies?: { count?: number; rows?: MeasureRow[] };
  storage?: {
    localStorageKeys?: string[];
    sessionStorageKeys?: string[];
    suspiciousLocalKeys?: string[];
    suspiciousSessionKeys?: string[];
    indexedDBDatabases?: unknown[];
    cacheNames?: unknown[];
    serviceWorkers?: unknown[];
  };
};

/** Artifact paths for one query; null where that artifact is absent. */
export type MeasurePaths = {
  dir: string | null;
  perf: string | null;
  net: string | null;
  storage: string | null;
  har: string | null;
};

/** One compact HAR entry row. */
export type HarRow = {
  index: number;
  method: string | undefined;
  url: string | undefined;
  host: string;
  status: number | undefined;
  ms: number;
  type: JsonScalar;
  failed: boolean;
};

export type PageInfo = {
  page: number;
  pageSize: number;
  totalRows: number;
  totalPages: number;
  hasMore: boolean;
};

export type QueryHint = { view: string; code?: string };

/** The lists and sections measure-query adds to its summary, by view. */
export type QueryExtras = {
  findings?: MeasureFinding[];
  failures?: MeasureRow[];
  slow?: { network: MeasureRow[]; resources: MeasureRow[] };
  sample?: MeasureRow[];
  resources?: MeasureRow[];
  measures?: unknown[];
  paints?: unknown;
  fcp?: unknown;
  lcp?: unknown;
  cls?: unknown;
  cookies?: MeasureRow[];
  keys?: Record<string, unknown[]>;
  har?: Record<string, unknown>;
  healthFilter?: Record<string, unknown>;
  hints?: QueryHint[];
  pagination?: Record<string, PageInfo>;
  next?: { continue?: { command: string; args: string[] } };
};
