# Repository layouts

Load when choosing folder organization, after identifying the required dependency rule. Names illustrate placement; build rules, lints and package boundaries enforce architecture.

| Workload | Example paths | Rule to verify |
|---|---|---|
| Straightforward API or CRUD | `src/controllers/`, `services/`, `repositories/`, `models/`, `dtos/`, `middleware/`, `config/`; compose in `main.ts` | Presentation calls business logic, which accesses data through its owned boundary. |
| Feature-based modular monolith | `src/modules/users/`, `orders/`, `payments/`; colocate each feature's API, logic, persistence and unit tests | Features expose a deliberate public surface. Keep `shared/` limited to genuinely shared ownership. |
| Clean or hexagonal domain | `src/domain/`, `application/use-cases/`, `application/ports/`, `infrastructure/`, `presentation/` | Infrastructure and presentation depend on application/domain abstractions; business policy does not import framework or storage types. |
| DDD with feature modules | `src/modules/orders/{domain,application,infrastructure,presentation}/`; repeat for distinct bounded contexts | Domain entities, value objects, events and repository interfaces belong to their context. Share only an intentional kernel. |
| Independently deployed services in one repository | `services/orders/`, `services/payments/`, `packages/contracts/`, `infrastructure/` | Deployment and data ownership are independent. Share stable contracts, never internal domain models. |
| Desktop application with IPC | `src/main/ipc/`, `main/services/`, `preload/`, `renderer/`, `shared/ipc-contracts.ts` | Renderer → narrow validated preload API → IPC handler → service. Privileges stay in the owning process. |
| Frontend with several features | `src/app/{routes,providers}/`, `features/auth/{components,hooks,api,types}/`, `components/ui/`, `assets/` | Feature ownership governs imports; shared UI does not acquire feature policy. |
| Engineering support | Colocated unit tests; `tests/{integration,e2e,fixtures}/`, `scripts/{dev,diagnostics}/`, `config/`, `docs/{architecture,adr,runbooks}/`, `.github/workflows/` | Give tests, configuration, docs and automation explicit owners. Preserve existing project conventions. |

Start with feature modules unless the workload justifies another layout. Locate composition roots and application entrypoints explicitly; avoid adding empty folders from an example.

Verify the actual rule with [wiring analysis](analysis.md#model-the-wiring): graphQuery `path` across the named boundaries, `issues` with `detectors: ["boundary-violation","dir-cycle"]`, and source/semantic confirmation. A folder-only claim remains `inferred`. Keep runtime evidence separate from dependency topology.
