// Compare evidence and executable requests; independent issued handles and
// retention timestamps are metadata, while source identities remain evidence.
import { contract } from '../contract.mjs';
import { canon, leadsIn, mapDeclared } from './calls.mjs';
import { dataOf, rowsOf } from './driver.mjs';
const CLI_ONLY = new Set(contract.tools.filter(tool => tool.cliOnly).map(tool => tool.name));
const EXIT = { invalidInput: 2, notFound: 3, authentication: 4, permission: 4, execution: 5, rateLimited: 7 };

export function transportPagesProblem(surface, responses) {
  const json = responses.length === 1 && !responses[0].json?.responsePagination
    ? responses[0].json : { results: responses.flatMap(response => rowsOf(response.json)) };
  for (const [index, response] of responses.entries()) {
    // Rejected admission rows appear only in the first response. Later CLI
    // requests resume the admitted work, whose failure class can differ.
    const outcome = surface === 'cli' && index > 0 ? response.json : json;
    const problem = transportProblem(surface, { ...response, json: outcome });
    if (problem) return problem;
  }
  return null;
}

export function transportProblem(surface, response) {
  const { json } = response;
  const rows = rowsOf(json);
  const failures = rows.filter(row => row.status === 'error' || row.data?.errorCode);
  // A display page cannot reveal siblings on other pages. The completed walk
  // checks the full logical batch with transportPagesProblem.
  if (json?.responsePagination && !json.errorCode) return partialTransportProblem(surface, response, rows, failures);
  if (surface === 'mcp') {
    const allFailed = allRowsFailed(json, rows, failures);
    return response.isError === allFailed ? null : `isError ${response.isError}, expected ${allFailed}`;
  }
  const classes = new Set([json?.errorCode, ...failures.map(row => row.data?.errorCode)].filter(Boolean).map(code => contract.errorCodes[code]));
  let exit = rows.length > 0 && rows.every(row => ['empty', 'outOfRange'].includes(row.status)) ? 1 : 0;
  if (classes.has('invalidInput')) exit = 2;
  else if (classes.size) exit = classes.size === 1 ? EXIT[[...classes][0]] : 5;
  return response.status === exit ? null : `exit ${response.status}, expected ${exit}`;
}

const allRowsFailed = (json, rows, failures) => !!json?.errorCode || rows.length > 0 && failures.length === rows.length;

function partialTransportProblem(surface, response, rows, failures) {
  if (surface === 'mcp') return response.isError && failures.length < rows.length ? 'isError true despite a successful batch row' : null;
  if (failures.some(row => row.data?.errorCode === 'invalidInput')) return response.status === 2 ? null : `exit ${response.status}, expected 2`;
  return [0, 1, 2, 3, 4, 5, 7].includes(response.status) ? null : `invalid exit ${response.status}`;
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
