// Advanced cases share the existing transport and continuation driver. Domain
// callbacks provide independent evidence expectations, never their own pager.
import { contract } from '../contract.mjs';
import { errorCodes } from './driver.mjs';
import { transportProblem } from './compare.mjs';

const schemas = new Map(contract.tools.map(tool => [tool.name, tool.outputSchema]));

export function advancedContext(run) {
  const { suite } = run;
  const callOn = surface => async (tool, input, options) => {
    const result = await run.callOn(surface)(tool, input, options);
    const index = run.advancedCalls.length;
    const codes = errorCodes(result.json);
    const invalid = suite.validateInput(tool, input);
    run.record(`advanced.input.${surface}.${index}`, !invalid || codes.includes('invalidInput'), invalid ?? 'published input accepted');
    const outputError = suite.validateOutput(tool, result.json);
    run.record(`advanced.output.${surface}.${index}`, !outputError, outputError ?? 'published output accepted');
    const transport = transportProblem(surface, result);
    run.record(`advanced.transport.${surface}.${index}`, !transport, transport ?? 'transport status matches outcome');
    run.advancedCalls.push({
      tool, surface, resume: typeof input?.cursor === 'string',
      queries: Array.isArray(input?.queries) ? input.queries.map(query => query && typeof query === 'object'
        ? { operation: query.operation ?? null, view: query.view ?? null, fields: Object.keys(query).toSorted() } : { invalid: true }) : undefined,
      errorCodes: codes,
    });
    return result;
  };
  const explore = async (tool, input, surface, options = {}) => {
    const responses = [];
    const report = await suite.driver.explore(tool, input, {
      surface, maxCalls: 1, maxPages: 200, ...options,
      call: async (name, request) => {
        const result = await callOn(surface)(name, request);
        responses.push({ tool: name, input: request, result });
        return result;
      },
    });
    return Object.assign(report, { responses });
  };
  const scenario = (name, surface, callback) => suite.surfaces.scenario({ tool: run.name, surface, cwd: run.cwd, name }, callback);
  return { scenario, driver: suite.driver, schemaOf: tool => schemas.get(tool), callOn, call: (input, surface) => callOn(surface)(run.name, input), cliCall: (tool, input, options) => callOn('cli')(tool, input, options), explore };
}
