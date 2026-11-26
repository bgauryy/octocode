Load when naming a messaging or distributed-system pattern (pub/sub, outbox, saga, dead letter queue) or a runtime mechanism (plugin loader, middleware pipeline, state machine, registry).

# Concepts: messaging, distributed patterns and runtime mechanics

## Messaging and distributed-system patterns

| Pattern | Description | When to use |
|---|---|---|
| Publish/subscribe | Publishers send events to subscribers through a channel; one event may reach multiple consumers | Event notifications, event-driven decoupling |
| Message router, content-based router | Routes messages by rules or by content | Integration pipelines, conditional routing |
| Message translator | Changes message formats | System integration |
| Message filter | Discards irrelevant messages | Event pipelines |
| Aggregator, splitter | Combines messages into one result; breaks one into units | Parallel processing, batch processing |
| Competing consumers | Several consumers read one queue | Horizontal scaling of work |
| Claim check | Store a large payload and pass a reference | Large messages |
| Dead letter queue (DLQ) | Stores messages that cannot be processed | Recovering failed message operations |
| Inbox | Tracks processed incoming message ids | Consumer deduplication |
| Idempotent consumer | Safely handles duplicate deliveries | At-least-once messaging |
| Choreography | Services react to each other's events | Decentralized workflows |
| Orchestration | A coordinator directs workflow steps | Explicit workflow management |
| Message broker | Intermediary that stores and delivers messages | Reliable asynchronous service integration |
| Load balancing | Distributes traffic across instances | Scaling and availability |
| Service discovery | Resolves available service instances | Dynamic distributed environments |
| Sidecar | Runs supporting functionality beside an application | Proxies, telemetry, service mesh; costs resources per instance |
| Ambassador | A proxy for outbound calls (retries, routing, credentials) | Legacy clients that cannot add resilience themselves; adds a network hop |
| Service mesh | Sidecar proxies plus a control plane that set service-to-service traffic, identity and policy | Many services needing uniform mTLS, retries and telemetry; infrastructure complexity |
| Distributed lock | Exclusive ownership across nodes | Rare shared-resource coordination |
| Sharding | Partitions data between nodes | Large datasets and high scale |
| Strong, eventual, causal consistency | Readers see the latest write; converge later; see writes in causal order | Pick per data item; name the window a consumer can observe |

Resilience patterns (timeout, retry with backoff and jitter, circuit breaker, bulkhead, rate limiting, health checks) are defined with their watch points in [reliability and overload](systems-reliability.md#reliability-and-overload); saga, transactional outbox, idempotency keys and leader election in [distributed data](data-systems-distributed.md#distributed-data-and-consistency).

## Runtime mechanics

| Pattern or concept | Description | When to use |
|---|---|---|
| Plugin system, plugin loader | Discovers and loads optional functionality | Extensible products |
| Middleware pipeline | Ordered request processing stages | Servers, CLI tools |
| State machine, finite state machine | Valid states and transitions | Protocols, workflows, UI flows |
| Hierarchical state machine | Nested states | Complex workflows |
| Rule engine | Evaluates configurable rules | Dynamic business policies |
| Workflow engine | Executes defined business processes | Long-running workflows |
| Dependency graph | Dependencies between components | Startup, build, and task ordering |
| Registry | Maps identifiers to implementations | Handler discovery; an expected hub |
| Context object | Carries request-scoped information | Request ids, deadlines, cancellation |
| Null object | Safe default behavior | Removing some null checks |
| Result (either), option (maybe) | Explicit success or failure; explicit absence | Recoverable errors; null-safe APIs |
| RAII, resource scope | Cleanup tied to object lifetime or a scope | Files, connections, locks |
| Graceful shutdown | Stop intake, drain in-flight work, release resources | Services and workers |
