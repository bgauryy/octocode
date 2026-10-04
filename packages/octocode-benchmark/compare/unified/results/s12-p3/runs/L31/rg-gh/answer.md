At commit 52fa3aef1f, Newtonsoft.Json builds two target frameworks that 13.0.3 didn't have and drops two that it did.

**Added:** `net8.0` and `net10.0`.

**Dropped:** `netstandard1.0` and `netstandard1.3`.

**Unchanged:** `net6.0`, `net45`, `net40`, `net35`, `net20` and `netstandard2.0`.

**Evidence**
- At the pinned commit, `Src/Newtonsoft.Json/Newtonsoft.Json.csproj:3` has `<TargetFrameworks ...>net10.0;net8.0;net6.0;net45;net40;net35;net20;netstandard2.0</TargetFrameworks>`.
- At the 13.0.3 tag, the same file's line 3 reads `net6.0;net45;net40;net35;net20;netstandard1.0;netstandard1.3;netstandard2.0`. I read it through `gh api`.

**Caveats**
- I compared the project file at the 13.0.3 tag, not the published NuGet package contents. I'm assuming the two match.
- Line 4 of the csproj lets a `LibraryFrameworks` property override the list. The lists above are the defaults.