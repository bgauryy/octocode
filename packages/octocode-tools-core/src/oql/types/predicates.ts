/**
 * OQL predicates — the boolean/leaf conditions a query's `where` clause is
 * built from (text/regex/structural/field matches plus all/any/not
 * composition), and the query source/scope shapes they operate over.
 */

export type PredicateId = string;

export type QuerySource =
  | { kind: 'local'; path: string }
  | { kind: 'github'; repo?: string; owner?: string; ref?: string }
  | { kind: 'materialized'; localPath: string; source?: QuerySource }
  | { kind: 'npm' };

export interface QueryScope {
  path?: string | string[];
  language?: string | string[];
  include?: string[];
  exclude?: string[];
  excludeDir?: string[];
  hidden?: boolean;
  noIgnore?: boolean;
  minDepth?: number;
  maxDepth?: number;
}

export interface TextPredicate {
  id?: PredicateId;
  kind: 'text';
  value: string;
  case?: 'smart' | 'sensitive' | 'insensitive';
  wholeWord?: boolean;
}

export interface RegexPredicate {
  id?: PredicateId;
  kind: 'regex';
  value: string;
  dialect?: 'rust' | 'pcre2' | 'provider';
  case?: 'smart' | 'sensitive' | 'insensitive';
  wholeWord?: boolean;
  multiline?: boolean;
  dotAll?: boolean;
}

export interface StructuralRule {
  pattern?: string;
  kind?: string;
  inside?: StructuralRule;
  has?: StructuralRule;
  not?: StructuralRule;
  all?: StructuralRule[];
  any?: StructuralRule[];
  stopBy?: 'end';
}

export type StructuralRuleInput = StructuralRule | string;

export interface StructuralPredicate {
  id?: PredicateId;
  kind: 'structural';
  lang: string;
  pattern?: string;
  rule?: StructuralRuleInput;
}

export type FieldName =
  | 'path'
  | 'basename'
  | 'extension'
  | 'size'
  | 'modified'
  | 'accessed'
  | 'empty'
  | 'permissions'
  | 'executable'
  | 'readable'
  | 'writable'
  | 'entryType';

export type FieldOp =
  | '='
  | '!='
  | 'in'
  | 'exists'
  | 'glob'
  | 'regex'
  | '>'
  | '>='
  | '<'
  | '<='
  | 'within'
  | 'before';

export interface FieldPredicate {
  id?: PredicateId;
  kind: 'field';
  field: FieldName;
  op: FieldOp;
  value?: unknown;
}

export interface AllPredicate {
  kind: 'all';
  id?: PredicateId;
  of: Predicate[];
}
export interface AnyPredicate {
  kind: 'any';
  id?: PredicateId;
  of: Predicate[];
}
export interface NotPredicate {
  kind: 'not';
  id?: PredicateId;
  predicate: Predicate;
}

export type LeafPredicate =
  TextPredicate | RegexPredicate | StructuralPredicate | FieldPredicate;

export type Predicate =
  AllPredicate | AnyPredicate | NotPredicate | LeafPredicate;
