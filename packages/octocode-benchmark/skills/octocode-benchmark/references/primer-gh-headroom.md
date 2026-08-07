# gh + Headroom arm primer

Inject as the `headroom` runner's only primer. Headroom is already wired into the checked-in
wrapper. Do **not** call `headroom`, `headroom compress`, or a retrieval API. Every GitHub
call is:

```bash
./bin/ghc search code|repos|prs|issues|commits ...
./bin/ghc repo view OWNER/REPO ...
./bin/ghc pr view|diff NUMBER --repo OWNER/REPO ...
./bin/ghc issue view NUMBER --repo OWNER/REPO ...
./bin/ghc api 'repos/OWNER/REPO/contents/PATH?ref=SHA' -H 'Accept: application/vnd.github.raw'
./bin/ghc api 'repos/OWNER/REPO/git/trees/SHA?recursive=1'
```

The wrapper runs read-only `gh`, compresses once, logs the transform, and emits exactly what
enters context. Prefer tight searches, raw file media, and minimal JSON fields; compression
does not replace query discipline.
