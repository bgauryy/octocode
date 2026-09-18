# Q9 — Fastify lifecycle

The documented sequence is: **Incoming Request → Routing → Instance Logger → onRequest → preParsing → Parsing → preValidation → Validation → preHandler → User Handler** (then Reply and the response phases). See [`Lifecycle.md`](https://github.com/fastify/fastify/blob/main/docs/Reference/Lifecycle.md).

In [`lib/route.js`](https://github.com/fastify/fastify/blob/main/lib/route.js), the route context property is `context.onRequest`; when non-null, Fastify calls `onRequestHookRunner(context.onRequest, request, reply, runPreParsing)`, otherwise it continues directly to `runPreParsing`.
