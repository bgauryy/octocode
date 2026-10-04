At commit 52fa3aef1f, Newtonsoft.Json builds **net10.0 and net8.0, which are new**, and **netstandard1.0 and netstandard1.3, which are dropped**.

- **Commit 52fa3aef1f:** `Src/Newtonsoft.Json/Newtonsoft.Json.csproj:3` sets `TargetFrameworks` to `net10.0;net8.0;net6.0;net45;net40;net35;net20;netstandard2.0`.
- **Release 13.0.3:** the same csproj at tag `13.0.3` (fetched with `gh api`) sets `net6.0;net45;net40;net35;net20;netstandard1.0;netstandard1.3;netstandard2.0`.
- **Unchanged:** net6.0, net45, net40, net35, net20 and netstandard2.0 appear in both lists.

I took the 13.0.3 list from the csproj at that tag, not from the NuGet package contents, so I didn't check what the published package actually contains. Line 4 of the csproj lets a `LibraryFrameworks` property override the list, so a custom build can differ from the default.