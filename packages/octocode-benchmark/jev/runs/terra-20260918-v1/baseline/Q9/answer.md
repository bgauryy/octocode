# Q9 — Fastify lifecycle

The documented sequence is: **Incoming Request → Routing → Instance Logger → onRequest → preParsing → Parsing → preValidation → Validation → preHandler → User Handler** ([Lifecycle diagram](https://github.com/fastify/fastify/blob/630acd0b6cf8a91322ff05c3d95feb991091866d/docs/Reference/Lifecycle.md#L10-L37)).

The per-route context property is `context.onRequest`; `Context` initializes it to `null` ([`lib/context.js`](https://github.com/fastify/fastify/blob/630acd0b6cf8a91322ff05c3d95feb991091866d/lib/context.js#L47-L64)). In [`lib/route.js`](https://github.com/fastify/fastify/blob/630acd0b6cf8a91322ff05c3d95feb991091866d/lib/route.js#L564-L574), a non-null value is invoked through `onRequestHookRunner(context.onRequest, request, reply, runPreParsing)`; otherwise it calls `runPreParsing` directly.
