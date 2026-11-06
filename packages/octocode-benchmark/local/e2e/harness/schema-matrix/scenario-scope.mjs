/** Keep provider overrides and all continuations inside the scenario's credential. */
export function withScenario(surfaces, options, responders, run) {
  return surfaces.scenario(options, async ({ token }) => {
    const { mock } = surfaces.ctx;
    mock.model.githubResponders.set(token, responders.github);
    mock.classificationResponders.set(token, responders.classification);
    try { return await run(); }
    finally {
      mock.model.githubResponders.delete(token);
      mock.classificationResponders.delete(token);
    }
  });
}
