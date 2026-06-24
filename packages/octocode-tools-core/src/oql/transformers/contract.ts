import type {
  OqlActiveTarget,
  OqlRecordResultRow,
  QuerySource,
} from '../types.js';

export type TransformerStatus = 'active' | 'planned';

export type TransformerBackend = {
  readonly backend: string;
  readonly operation: string;
  readonly exact: boolean;
};

export type TransformerRegistryEntry = {
  readonly id: string;
  readonly target: OqlActiveTarget;
  readonly sourceKinds: readonly QuerySource['kind'][];
  readonly status: TransformerStatus;
  readonly backends: readonly TransformerBackend[];
  readonly rowKind: 'code' | 'file' | 'tree' | 'content' | 'record';
  readonly recordType?: OqlRecordResultRow['recordType'];
  readonly adapterModule?: string;
  readonly adapterFunctions: readonly string[];
  readonly notes?: readonly string[];
};
