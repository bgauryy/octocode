# Q3 — Flask route history

Current `route` is `Scaffold.route` in [`src/flask/sansio/scaffold.py`](https://github.com/pallets/flask/blob/main/src/flask/sansio/scaffold.py); `Scaffold` is the shared base for `Flask` and `Blueprint`.

The changed code in [`705e5268`](https://github.com/pallets/flask/commit/705e52684a9063889c16a289695a2e4429df6887) added `_method_route`, which rejects an explicit `methods` option and delegates to `self.route(rule, methods=[method], **options)`. It then added `get`, `post`, `put`, `delete`, and `patch` decorators and tests invoking each. Thus it introduced convenience verb decorators—e.g. `@app.post(...)`—rather than a new underlying routing/registration mechanism.
