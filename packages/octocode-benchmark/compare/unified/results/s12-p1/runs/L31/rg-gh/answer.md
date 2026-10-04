At commit 52fa3aef1f, Newtonsoft.Json adds **net8.0** and **net10.0** and drops **netstandard1.0** and **netstandard1.3**, compared with 13.0.3.

- **13.0.3:** `Src/Newtonsoft.Json/Newtonsoft.Json.csproj:3` (read from the `13.0.3` tag through `gh api`) lists `net6.0;net45;net40;net35;net20;netstandard1.0;netstandard1.3;netstandard2.0`.
- **Pinned commit:** `Src/Newtonsoft.Json/Newtonsoft.Json.csproj:3` in the local checkout lists `net10.0;net8.0;net6.0;net45;net40;net35;net20;netstandard2.0`.

**Added:**
- net10.0
- net8.0

These also get their own property group at line 61.

**Dropped:**
- netstandard1.0
- netstandard1.3

**Unchanged:**
- net6.0
- net45
- net40
- net35
- net20
- netstandard2.0

**Caveat:** I read the 13.0.3 project file from the repo tag, not from the published NuGet package contents. Line 3 is the default list. Setting the `LibraryFrameworks` property overrides it, as line 4 shows.