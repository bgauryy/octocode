# Local root-cause scenarios: 30

Each file is a symptom and a few open questions on one pinned clone (path `packages/octocode-benchmark/local/repos/<dir>`;
commits in [local/README.md](../../local/README.md)). There is no answer key: the judge works out the causal chain
from the source at the pinned commit and checks every cited line. Use them as exploratory research (flow 15 of
[local/FLOWS.md](../../local/FLOWS.md)), not as one fresh agent per question with a score each.

| Scenario | Title | Clone dir |
|---|---|---|
| [01-asm-simd-dispatch-and-unsupported-instructions.md](01-asm-simd-dispatch-and-unsupported-instructions.md) | SIMD dispatch and unsupported instructions | `asm` |
| [02-asm-vector-tails-and-exact-buffer-boundaries.md](02-asm-vector-tails-and-exact-buffer-boundaries.md) | Vector tails and exact buffer boundaries | `asm` |
| [03-c-redis-slow-client-backpressure.md](03-c-redis-slow-client-backpressure.md) | Redis slow-client backpressure | `c` |
| [04-c-aof-acknowledgement-versus-durable-recovery.md](04-c-aof-acknowledgement-versus-durable-recovery.md) | AOF acknowledgement versus durable recovery | `c` |
| [05-cpp-json-parse-failure-and-exception-policy.md](05-cpp-json-parse-failure-and-exception-policy.md) | JSON parse failure and exception policy | `cpp` |
| [06-cpp-json-depth-and-allocation-amplification.md](06-cpp-json-depth-and-allocation-amplification.md) | JSON depth and allocation amplification | `cpp` |
| [07-csharp-reference-preservation-and-object-identity.md](07-csharp-reference-preservation-and-object-identity.md) | Reference preservation and object identity | `csharp` |
| [08-csharp-deserialization-conversion-error-recovery.md](08-csharp-deserialization-conversion-error-recovery.md) | Deserialization conversion error recovery | `csharp` |
| [09-go-scrape-cancellation-and-sample-commit.md](09-go-scrape-cancellation-and-sample-commit.md) | Scrape cancellation and sample commit | `go` |
| [10-go-target-reload-and-duplicate-scrape-loops.md](10-go-target-reload-and-duplicate-scrape-loops.md) | Target reload and duplicate scrape loops | `go` |
| [11-java-concurrent-cache-loading-and-invalidation.md](11-java-concurrent-cache-loading-and-invalidation.md) | Concurrent cache loading and invalidation | `java` |
| [12-java-cache-expiration-versus-refresh.md](12-java-cache-expiration-versus-refresh.md) | Cache expiration versus refresh | `java` |
| [13-javascript-lazy-sequence-shortcut-eligibility.md](13-javascript-lazy-sequence-shortcut-eligibility.md) | Lazy sequence shortcut eligibility | `javascript` |
| [14-javascript-deep-merge-key-safety-and-inherited-properties.md](14-javascript-deep-merge-key-safety-and-inherited-properties.md) | Deep merge key safety and inherited properties | `javascript` |
| [15-python-orm-joins-change-aggregate-semantics.md](15-python-orm-joins-change-aggregate-semantics.md) | ORM joins change aggregate semantics | `python` |
| [16-python-atomic-nesting-and-commit-callbacks.md](16-python-atomic-nesting-and-commit-callbacks.md) | Atomic nesting and commit callbacks | `python` |
| [17-rust-tokio-wakeups-and-apparent-scheduler-stalls.md](17-rust-tokio-wakeups-and-apparent-scheduler-stalls.md) | Tokio wakeups and apparent scheduler stalls | `rust` |
| [18-rust-task-cancellation-and-destructor-timing.md](18-rust-task-cancellation-and-destructor-timing.md) | Task cancellation and destructor timing | `rust` |
| [19-scala-eval-stack-safety-across-mixed-evaluation-modes.md](19-scala-eval-stack-safety-across-mixed-evaluation-modes.md) | Eval stack safety across mixed evaluation modes | `scala` |
| [20-scala-traverse-short-circuiting-and-evaluation-order.md](20-scala-traverse-short-circuiting-and-evaluation-order.md) | Traverse short-circuiting and evaluation order | `scala` |
| [21-tsx-undo-boundaries-under-rapid-ui-updates.md](21-tsx-undo-boundaries-under-rapid-ui-updates.md) | Undo boundaries under rapid UI updates | `tsx` |
| [22-tsx-restored-scene-data-and-rendering-identity.md](22-tsx-restored-scene-data-and-rendering-identity.md) | Restored scene data and rendering identity | `tsx` |
| [23-huge-ts-extension-activation-dependency-failure.md](23-huge-ts-extension-activation-dependency-failure.md) | Extension activation dependency failure | `huge-ts` |
| [24-huge-ts-cancellation-across-extension-host-rpc.md](24-huge-ts-cancellation-across-extension-host-rpc.md) | Cancellation across extension-host RPC | `huge-ts` |
| [25-huge-go-deployment-rollout-convergence-under-informer-delay.md](25-huge-go-deployment-rollout-convergence-under-informer-delay.md) | Deployment rollout convergence under informer delay | `huge-go` |
| [26-langchain-runnable-stream-failure-and-retry-scope.md](26-langchain-runnable-stream-failure-and-retry-scope.md) | Runnable stream failure and retry scope | `langchain` |
| [27-nextjs-suspense-streaming-and-late-server-failures.md](27-nextjs-suspense-streaming-and-late-server-failures.md) | Suspense streaming and late server failures | `nextjs` |
| [28-nextjs-revalidation-and-stale-response-races.md](28-nextjs-revalidation-and-stale-response-races.md) | Revalidation and stale response races | `nextjs` |
| [29-terraform-partial-apply-and-state-persistence.md](29-terraform-partial-apply-and-state-persistence.md) | Partial apply and state persistence | `terraform` |
| [30-typescript-typescript-checker-pool-reuse-and-isolation.md](30-typescript-typescript-checker-pool-reuse-and-isolation.md) | TypeScript checker-pool reuse and isolation | `typescript` |
