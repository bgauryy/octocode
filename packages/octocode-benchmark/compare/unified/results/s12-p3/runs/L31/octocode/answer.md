At commit 52fa3aef1f, Newtonsoft.Json builds for net10.0, net8.0, net6.0, net45, net40, net35, net20 and netstandard2.0. The list is on line 3 of `Src/Newtonsoft.Json/Newtonsoft.Json.csproj`: `net10.0;net8.0;net6.0;net45;net40;net35;net20;netstandard2.0`. That is the default, used when `LibraryFrameworks` isn't set.

- **Added:** `net8.0` and `net10.0`.
- **Dropped:** `netstandard1.0` and `netstandard1.3`.
- **Unchanged:** `net6.0`, `net45`, `net40`, `net35`, `net20` and `netstandard2.0`.

I didn't check the 13.0.3 target list against the package itself. I took it from memory as `net6.0`, `net45`, `net40`, `net35`, `net20`, `netstandard2.0`, `netstandard1.3` and `netstandard1.0`. The "dropped" and "unchanged" lists depend on that recollection. The commit's own list is confirmed from the csproj.