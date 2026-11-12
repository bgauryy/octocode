Load when choosing folder organization, after identifying the required dependency rule. Examples illustrate layouts; names alone do not enforce an architecture.

# Repository layouts

A folder layout is a layering candidate, `inferred` until a build rule, lint, or package boundary enforces it ([model the wiring](analysis.md#model-the-wiring)). Verify a layout's claim with graphQuery `path` across the folders it names, and `issues` with `detectors: ["boundary-violation","dir-cycle"]`.

Layered, for straightforward APIs and CRUD applications:

```text
my-app/
├── src/
│   ├── controllers/user.controller.ts
│   ├── services/user.service.ts
│   ├── repositories/user.repository.ts
│   ├── models/user.model.ts
│   ├── dtos/
│   ├── middleware/
│   ├── config/
│   └── main.ts
├── tests/
└── package.json
```

Feature-based modular monolith, the default for most medium-sized projects:

```text
my-app/
├── src/
│   ├── modules/
│   │   ├── users/
│   │   │   ├── user.controller.ts
│   │   │   ├── user.service.ts
│   │   │   ├── user.repository.ts
│   │   │   ├── user.model.ts
│   │   │   └── user.test.ts
│   │   ├── orders/
│   │   └── payments/
│   ├── shared/
│   │   ├── errors/
│   │   ├── logging/
│   │   └── config/
│   └── main.ts
├── tests/
└── package.json
```

Clean or hexagonal, for complex business logic with several infrastructure integrations. Infrastructure and presentation depend on application and domain abstractions, never the reverse:

```text
my-app/
├── src/
│   ├── domain/
│   │   ├── entities/
│   │   ├── value-objects/
│   │   ├── services/
│   │   └── events/
│   ├── application/
│   │   ├── use-cases/
│   │   ├── ports/
│   │   └── dtos/
│   ├── infrastructure/
│   │   ├── persistence/
│   │   ├── messaging/
│   │   └── external-apis/
│   ├── presentation/
│   │   ├── controllers/
│   │   └── middleware/
│   └── main.ts
├── tests/
└── package.json
```

DDD with feature modules, for large business domains:

```text
my-app/
├── src/
│   ├── modules/
│   │   ├── orders/
│   │   │   ├── domain/        # entities, value-objects, events, repositories (interfaces)
│   │   │   ├── application/   # commands, queries, handlers
│   │   │   ├── infrastructure/ # persistence, messaging
│   │   │   └── presentation/  # controllers
│   │   ├── payments/
│   │   └── inventory/
│   ├── shared/
│   └── main.ts
└── tests/
```

Microservices in a monorepo, when services genuinely need independent deployment. Share stable contracts, never internal domain models:

```text
platform/
├── services/
│   ├── user-service/
│   │   ├── src/
│   │   ├── tests/
│   │   └── Dockerfile
│   ├── order-service/
│   └── payment-service/
├── packages/
│   ├── contracts/
│   ├── observability/
│   └── tooling/
├── infrastructure/
│   ├── docker/
│   ├── kubernetes/
│   └── terraform/
├── scripts/
└── README.md
```

Desktop application with IPC (Electron, Tauri, multi-process). The flow is renderer, preload API, IPC handler, service; expose a narrow, validated IPC API instead of privileged access:

```text
desktop-app/
├── src/
│   ├── main/
│   │   ├── ipc/file.handler.ts
│   │   ├── services/file.service.ts
│   │   └── main.ts
│   ├── preload/index.ts
│   ├── renderer/
│   │   ├── components/
│   │   ├── features/
│   │   ├── hooks/
│   │   └── app.tsx
│   └── shared/
│       ├── ipc-contracts.ts
│       └── types.ts
├── tests/
└── package.json
```

Frontend with several features (React, Vue, Angular):

```text
frontend/
├── src/
│   ├── app/
│   │   ├── routes/
│   │   └── providers/
│   ├── features/
│   │   ├── auth/
│   │   │   ├── components/
│   │   │   ├── hooks/
│   │   │   ├── api/
│   │   │   └── types.ts
│   │   └── dashboard/
│   ├── components/ui/
│   ├── lib/
│   ├── assets/
│   └── main.tsx
├── public/
└── package.json
```

Engineering support, whatever the architecture: explicit homes for tests, configuration, documentation, diagnostics, and automation:

```text
project/
├── src/
├── tests/
│   ├── unit/
│   ├── integration/
│   ├── e2e/
│   └── fixtures/
├── scripts/
│   ├── dev/
│   └── diagnostics/
├── config/
├── docs/
│   ├── architecture/
│   ├── adr/
│   └── runbooks/
├── .github/workflows/
├── .vscode/launch.json
├── .env.example
├── Dockerfile
├── README.md
└── CONTRIBUTING.md
```

A practical default combines feature-based modules, enforced boundaries, colocated unit tests, centralized integration and end-to-end tests, structured logging, correlation ids, and reproducible debugging configurations.
