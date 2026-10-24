import assert from 'node:assert/strict';
import { classificationAnswer } from './mock-routes.mjs';
import { tokenFor } from './surfaces.mjs';

export const data = result => result.json?.results?.[0]?.data;
export const rowData = result => (result.json?.results ?? []).map(row => row.data);
export const noul = instructions => ({ type: 'noul', instructions });
export const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

/** One tool/surface credential owns its responder and observed provider packets. */
export function providerSession(ctx, tool, surface) {
  const session = { sent: [], respond: request => ({ body: classificationAnswer(request) }) };
  const token = tokenFor(tool, surface);
  ctx.mock.classificationResponders.set(token, (request, headers) => {
    session.sent.push({ request, headers });
    return session.respond(request, headers);
  });
  return { ...session, sent: session.sent,
    set: respond => { session.respond = respond; },
    close: () => ctx.mock.classificationResponders.delete(token),
  };
}

/** Use the shared driver; literal judgments have only display continuations. */
export async function display(ctx, surface, tool, input) {
  const report = await ctx.explore(tool, input, surface);
  assert.equal(report.failures.length, 0, JSON.stringify(report.failures));
  const results = report.responses.filter(entry => entry.tool === tool).map(entry => entry.result);
  return { results, rows: results.flatMap(rowData), first: data(results[0]) };
}

export async function readCaptured(ctx, surface, call) {
  assert.equal(call.kind, 'continue');
  assert.deepEqual(Object.keys(call.input), ['cursor']);
  const report = await ctx.explore(call.tool, call.input, surface);
  assert.equal(report.failures.length, 0, JSON.stringify(report.failures));
  return report.responses.filter(entry => entry.tool === call.tool).flatMap(entry => rowData(entry.result));
}

export const PNG = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAARgAAAEYCAAAAACi5bZQAAABXElEQVR42u3QAQ0AAAwCIPuX1gJP8EEEUk5RIEaMGDFixIgRI0aMGMSIESNGjBgxYsSIEYMYMWLEiBEjRowYMWIQI0aMGDFixIgRI0YMYsSIESNGjBgxYsSIEYMYMWLEiBEjRowYMWIQI0aMGDFixIgRI0YMYsSIESNGjBgxYsSIQYwYMWLEiBEjRowYMYgRI0aMGDFixIgRI0YMYsSIESNGjBgxYsSIQYwYMWLEiBEjRowYMYgRI0aMGDFixIgRIwYxYsSIESNGjBgxYsSIQYwYMWLEiBEjRowYMYgRI0aMGDFixIgRIwYxYsSIESNGjBgxYsQgRowYMWLEiBEjRowYxIgRI0aMGDFixIgRIwYxYsSIESNGjBgxYsQgRowYMWLEiBEjRowYxIgRI0aMGDFixIgRgxgxYsSIESNGjBgxYsQoECNGjBgxYsSIESNGDGLEiBEjRowYMWK+G5X/H6AJTpTJAAAAAElFTkSuQmCC', 'base64');
