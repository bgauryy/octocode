/**
 * OQL typed record-row aliases — a record row narrowed to a specific
 * `recordType` + matching `data` shape, plus the overall `OqlResultRow` union
 * and its proof-graded variant.
 */

import type {
  OqlCodeResultRow,
  OqlContentResultRow,
  OqlFileResultRow,
  OqlProofGrade,
  OqlRecordResultRow,
  OqlTreeResultRow,
} from './results.js';
import type {
  OqlCommitData,
  OqlDiffData,
  OqlGraphData,
  OqlMaterializedData,
  OqlPackageData,
  OqlPullRequestData,
  OqlRepositoryData,
  OqlResearchData,
  OqlSemanticsData,
} from './recordData.js';

/** Typed row aliases — a record row whose `data` matches its `recordType`. */
export type OqlRepositoryRow = OqlRecordResultRow & {
  recordType: 'repository';
  data: OqlRepositoryData;
};
export type OqlPackageRow = OqlRecordResultRow & {
  recordType: 'package';
  data: OqlPackageData;
};
export type OqlPullRequestRow = OqlRecordResultRow & {
  recordType: 'pullRequest';
  data: OqlPullRequestData;
};
export type OqlCommitRow = OqlRecordResultRow & {
  recordType: 'commit';
  data: OqlCommitData;
};
export type OqlDiffRow = OqlRecordResultRow & {
  recordType: 'diff';
  data: OqlDiffData;
};
export type OqlSemanticsRow = OqlRecordResultRow & {
  recordType: 'semantics';
  data: OqlSemanticsData;
};
export type OqlMaterializedRow = OqlRecordResultRow & {
  recordType: 'materialized';
  data: OqlMaterializedData;
};
export type OqlResearchRow = OqlRecordResultRow & {
  recordType: 'research';
  data: OqlResearchData;
};
export type OqlGraphRow = OqlRecordResultRow & {
  recordType: 'graph';
  data: OqlGraphData;
};

export type OqlResultRow =
  | OqlCodeResultRow
  | OqlFileResultRow
  | OqlTreeResultRow
  | OqlContentResultRow
  | OqlRecordResultRow;

export type OqlProofGradedResultRow = OqlResultRow & {
  proofGrade: OqlProofGrade;
};
