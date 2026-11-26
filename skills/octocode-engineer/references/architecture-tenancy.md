Load when several tenants share a system: choosing pool, bridge or silo isolation, or reviewing tenant separation.

# Architecture: multi-tenancy

## Multi-tenant architecture

Silo, bridge and pool are cost-versus-isolation choices; the model can differ per tier (shared compute, dedicated database).

| Model or pattern | Key consideration |
|---|---|
| Pool model | Shared resources and data structures; cheapest; isolation rests on code and policy |
| Bridge model | Partially separated tenant resources, such as a shared service with a schema per tenant |
| Silo model | Dedicated tenant resources; strongest isolation; highest cost and slowest onboarding |
| Tenant-aware routing | Route each request to the tenant's resources |
| Tenant context propagation | Carry tenant identity through every call, job and message |
| Row-level security | Restrict queries to the tenant's rows in the database itself |
| Per-tenant encryption keys | Separate cryptographic control and deletion by key destruction |
| Noisy-neighbor isolation, tenant rate limiting | Keep one tenant from consuming shared capacity; fair allocation |
| Tenant provisioning and offboarding | Automate onboarding; safe export, deletion and retention at exit |
| Tenant data migration | Move a tenant between deployments, for example from pool to silo |
| Tenant-specific configuration | Controlled customization without forking code |
| Tenant observability | Per-tenant latency, errors and cost |

Physical partitioning alone does not guarantee authorization isolation: check that every query and cache key carries the tenant and that the server derives the tenant from the authenticated identity, never from request input alone.
