At commit 52fa3aef1f, Newtonsoft.Json builds for **net10.0, net8.0, net6.0, net45, net40, net35, net20 and netstandard2.0**. Compared with 13.0.3, it adds net8.0 and net10.0 and drops netstandard1.0 and netstandard1.3.

- **Current set:** the default `TargetFrameworks` at `Src/Newtonsoft.Json/Newtonsoft.Json.csproj:3` is `net10.0;net8.0;net6.0;net45;net40;net35;net20;netstandard2.0`. Line 4 only swaps in `$(LibraryFrameworks)` when that property is set.
- **Added:** net8.0 and net10.0. Line 61 has a property group for them, and line 92 has one for compatibility with net8.0.
- **Dropped:** netstandard1.0 and netstandard1.3. Neither appears anywhere in the csproj target list.
- **Unchanged:** net6.0, net45, net40, net35, net20 and netstandard2.0.

The 13.0.3 list is from my memory of its NuGet package, not from anything I fetched. I recall it as net6.0, net45, net40, net35, net20, netstandard2.0, netstandard1.3 and netstandard1.0. I did not check that against the 13.0.3 tag or the package itself, so the "added" and "dropped" lists rest on that recollection.