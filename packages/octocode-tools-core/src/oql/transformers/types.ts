import type { OqlDiagnostic } from '../types.js';

export type OqlVendor = 'github' | 'local' | 'npm' | 'lsp' | 'binary';

export type TransformOk<TQuery extends Record<string, unknown>> = {
  ok: true;
  query: TQuery;
  diagnostics: OqlDiagnostic[];
};

export type TransformBlocked = {
  ok: false;
  diagnostics: OqlDiagnostic[];
};

export type TransformResult<TQuery extends Record<string, unknown>> =
  | TransformOk<TQuery>
  | TransformBlocked;
