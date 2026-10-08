# Test repositories (not committed)

The unified agent benchmark (`packages/octocode-benchmark/compare/unified`) and the harness suites run against shallow clones of real repositories. The clones are gitignored; only this file is tracked. Recreate them at the pinned commits so results stay comparable:

```sh
cd octocode-local-testing/repos
clone() { git init -q "$1" && git -C "$1" remote add origin "https://github.com/$2.git" \
  && git -C "$1" fetch -q --depth 1 origin "$3" && git -C "$1" checkout -q FETCH_HEAD; }
```

Then run the rows you need, for example `clone rust tokio-rs/tokio facc6fc`. If a short SHA can't be fetched, use `git clone --depth 1 https://github.com/<repo>.git <dir>` and note the new HEAD in your results.

| dir | repo | pinned commit | used for |
|---|---|---|---|
| typescript | microsoft/TypeScript | 4f5ddae2 | TypeScript (+ Go in the TS 7 port) |
| tsx | excalidraw/excalidraw | f3d99c4 | TSX |
| javascript | lodash/lodash | 2b5e6f7 | JavaScript |
| python | django/django | 4fab678 | Python |
| go | prometheus/prometheus | ea95480 | Go |
| rust | tokio-rs/tokio | facc6fc | Rust |
| java | google/guava | 4d41665 | Java |
| c | redis/redis | 20bb2cf | C |
| cpp | nlohmann/json | f422b75 | C++ |
| csharp | JamesNK/Newtonsoft.Json | 52fa3ae | C# |
| scala | typelevel/cats | 4a2ea73 | Scala |
| asm | libjpeg-turbo/libjpeg-turbo | 03c9a9d | Assembly |
| huge-ts | microsoft/vscode | 43dd9070 | huge-repo suites (optional) |
| huge-go | kubernetes/kubernetes | 6c1c7702 | huge-repo suites (optional) |
| huge-java | elastic/elasticsearch | 9438d0a6 | huge-repo suites (optional) |
| huge-rust | rust-lang/rust | b373574ee | huge-repo suites (optional) |
| huge-cpp | pytorch/pytorch | 03943c7 | huge-repo suites (optional) |
| huge-c | torvalds/linux | fd179f8a0 | huge-repo suites (optional) |
| langchain | langchain-ai/langchain | 67ee6cb63dd9ae7f3a4dfedc3095652bce15a125 | agent benchmark (`packages/octocode-benchmark/compare/unified`) |
| nextjs | vercel/next.js | d155ba9ebfffe4742efefda8d68c2e0e8e490924 | agent benchmark (`packages/octocode-benchmark/compare/unified`) |

The first 12 cover every grammar and are what the harness suites (`harness/*.mjs`) need (about 5 GB with the huge ones). Language servers are optional: suites record `serverUnavailable` rather than fail.
