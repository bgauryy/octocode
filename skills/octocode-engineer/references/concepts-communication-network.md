Load when naming a network or distributed transport: REST, GraphQL, gRPC, WebSocket, SSE, polling, webhooks or message streaming.

# Concepts: network and distributed communication

Transport choice and delivery guarantees are in [communication](concepts-communication.md#communication); pub/sub and message brokers in [messaging patterns](concepts-messaging.md#messaging-and-distributed-system-patterns).

## Network and distributed communication

| Technology or pattern | Description | When to use |
|---|---|---|
| REST | Resource-oriented HTTP interface style | Public APIs, CRUD operations |
| GraphQL | Clients query the data they need | Data-rich frontends |
| gRPC | Contract-based remote procedure calls | Internal, strongly typed service communication |
| JSON-RPC | Lightweight RPC protocol | Tools, internal APIs, local IPC |
| XML-RPC | RPC encoded as XML | Legacy integrations |
| SOAP | XML-based messaging protocol | Enterprise and legacy systems |
| WebSocket | Persistent bidirectional connection | Real-time messaging |
| Server-sent events (SSE) | Server-to-client streaming over HTTP | Live dashboards and progress updates |
| WebRTC data channels | Peer-to-peer data transport | Interactive peer communication |
| HTTP polling | Periodic requests for updated state | Simple or low-frequency updates |
| Long polling | Server holds a request until data is available | Notifications without persistent sockets |
| Request/response | Sender requests an operation and receives a result | Synchronous queries |
| Fire-and-forget | Sender does not wait for completion | Noncritical asynchronous notifications |
| Request/reply messaging | Correlated request and response messages | Asynchronous RPC-like workflows |
| Message streaming | Continuous ordered record processing | Analytics, pipelines, event processing |
| Webhooks | HTTP callbacks on events | Integrations with external platforms |
