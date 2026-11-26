Load when the question is about overload, retries, rate limits, capacity, API contracts, security and identity, testing, observability and SRE, deployment, code design or debugging method.

Read next: [reliability and rate limiting](systems-reliability.md) for overload, retries and rate limits; [capacity](systems-capacity.md) for estimates; [API contracts](systems-api-contracts.md) for interface evolution; [security](systems-security.md) for threats, identity and cryptography; [observability, SRE and delivery](systems-operations.md) for SLOs, telemetry and rollout.

# Systems and delivery

## Choose quickly

| Problem | Start with | Watch |
|---|---|---|
| A dependency is slow or down | Timeout + bounded retries with jittered backoff + circuit breaker | Retry only idempotent operations or ones carrying an idempotency key |
| Traffic exceeds capacity | Admission control: rate limit, concurrency limit, load shedding | Reject early and cheaply; a queue absorbs bursts the consumer can catch up on, but sustained overload needs a bound and shedding |
| Bursty clients | Token bucket | The bucket size is the allowed burst |
| Schema or API change in production | Expand–contract migration | Old and new versions must run side by side |
| Risky deploy | Feature flag or canary with automatic rollback | Define the rollback signal before the rollout |
| Unknown production behaviour | RED metrics per service, USE metrics per resource, traces across calls | Alert on symptoms users feel, not on every cause |
| A bug you cannot explain | Reproduce, minimize, bisect | Change one variable per experiment |

## Testing and verification

Test types, doubles and the flaky-test rule are in [testing concepts](concepts-bugs-testing.md#testing-concepts). Pick by the defect you fear: property-based tests state invariants (round-trip, idempotence, ordering) and let the tool search; load and soak tests need production-like data and traffic shape. Mock only external boundaries.

## Code design

| Principle | Meaning |
|---|---|
| Single source of truth | One owner for each fact, setting and rule; derive the rest |
| Make illegal states unrepresentable | Encode invariants in types instead of comments and checks |
| Parse, don't validate | Convert raw input once at the boundary into types that carry the guarantee |
| [Functional core, imperative shell](concepts-styles-more.md#more-architecture-styles) | Pure decisions inside, side effects at the edges |
| Fail loudly at boundaries | Surface errors with context; never swallow them or return success-shaped fallbacks |

Cohesion, coupling and dependency direction: [dependencies and coupling](concepts-coupling.md#dependencies-and-coupling). Module boundaries and refactors: [analysis](analysis.md) and [change discipline](change-discipline.md).

## Debugging method

1. **Reproduce** reliably, with the exact input, version and environment.
2. **Read** the whole error and the first failure, not the last.
3. **Minimize** the input or the code until the failure is small.
4. **Bisect** in history (`git bisect`), data or code paths.
5. **Hypothesize and test** one variable at a time; write down what each result rules out.
6. **Check assumptions** about versions, configuration, environment and which binary actually runs.
7. **Fix the cause** with a regression test that failed first, then look for the same bug elsewhere.

Tools and bug classes: [debugging and observability concepts](concepts-debugging.md#debugging-and-observability-concepts) and [bug categories](concepts-bugs-testing.md#bug-categories). To run the investigation on a live program (debugger, profiler, trace, dump), use `octocode-debugging` when installed.
