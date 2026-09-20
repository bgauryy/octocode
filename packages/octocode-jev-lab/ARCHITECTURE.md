# Jev lab architecture

`@octocodeai/jev-lab` is a private development probe, not a production tool or
public contract owner. It calls the TypeSafe System One endpoint directly so
provider behavior, answer shapes, token use, and latency can be checked without
building Octocode.

```text
JSON manifest
  ├─ provider-ready state, or bounded local resources
  └─ native TypeSafe questions
          │
          ▼
  request preparation + receipts
          │
          ▼
  repeated direct HTTPS requests
          │
          ▼
  unchanged provider JSON + adjacent measurements
```

Invariants:

- Credentials use the shared trusted Octocode environment loader and are never
  logged; shell values win and project `.env` files are ignored.
- Only HTTPS API roots are accepted; HTTP is limited to loopback tests.
- Local paths are used for loading but are not included in provider state.
- Resource-size failures are explicit; content is never silently truncated.
- Noul, Choice, Score, model, usage, and any future provider response fields
  remain unchanged under `samples[].response`.
- This package must not duplicate Octocode runtime policy or become a release
  dependency.
