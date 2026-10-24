// Compare evidence and executable requests; independent issued handles and
// retention timestamps are metadata, while source identities remain evidence.
import { contract } from '../contract.mjs';
import { canon, leadsIn, mapDeclared } from './calls.mjs';
import { dataOf, rowsOf } from './driver.mjs';
const CLI_ONLY = new Set(contract.tools.filter(tool => tool.cliOnly).map(tool => tool.name));
const EXIT = { invalidInput: 2, notFound: 3, authentication: 4, permission: 4, execution: 5, rateLimited: 7 };

export function transportProblem(surface, response) {
  const { json } = response;
  const rows = rowsOf(json);
  const failures = rows.filter(row => row.status === 'error' || row.data?.errorCode);
  if (surface === 'mcp') {
    const allFailed = !!json?.errorCode || rows.length > 0 && failures.length === rows.length;
    return response.isError === allFailed ? null : `isError ${response.isError}, expected ${allFailed}`;
  }
  const classes = new Set([json?.errorCode, ...failures.map(row => row.data?.errorCode)].filter(Boolean).map(code => contract.errorCodes[code]));
  let exit = rows.length > 0 && rows.every(row => ['empty', 'outOfRange'].includes(row.status)) ? 1 : 0;
  if (classes.has('invalidInput')) exit = 2;
  else if (classes.size) exit = classes.size === 1 ? EXIT[[...classes][0]] : 5;
  return response.status === exit ? null : `exit ${response.status}, expected ${exit}`;
}

export function firstError(json) {
  const error = json?.errorCode ? json : dataOf(rowsOf(json).find(row => row.status === 'error' || row.data?.errorCode));
  return error ? `${error.errorCode}: ${String(error.error ?? '').slice(0, 140)}` : '';
}

export function parityView(json, tool) {
  const schema = contract.tools.find(entry => entry.name === tool)?.outputSchema;
  const calls = new Map(leadsIn(json, schema).map(call => [JSON.stringify(call.segments), call]));
  return mapDeclared(json, schema, (value, path) => {
    const call = calls.get(JSON.stringify(path));
    if (call) {
      if (CLI_ONLY.has(call.tool)) return undefined;
      return call.kind === 'continue' ? { kind: call.kind, tool: call.tool, input: { cursor: '<issued>' } } : value;
    }
    if (path.at(-1) === 'hints') return undefined;
    if (path.at(-1) === 'expiresAt') return '<retention deadline>';
    return value;
  });
}

export function firstDiff(a, b, at = '') {
  if (canon(a) === canon(b)) return null;
  if (a && b && typeof a === 'object' && typeof b === 'object') {
    for (const key of new Set([...Object.keys(a), ...Object.keys(b)])) {
      const diff = firstDiff(a[key], b[key], `${at}/${key}`);
      if (diff) return diff;
    }
  }
  return `${at || '/'}: cli=${JSON.stringify(a)?.slice(0, 80)} mcp=${JSON.stringify(b)?.slice(0, 80)}`;
}
