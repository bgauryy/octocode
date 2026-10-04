At commit 52fa3aef1f, Newtonsoft.Json adds **net8.0** and **net10.0** and drops **netstandard1.0** and **netstandard1.3**, compared with 13.0.3.

**Current build:** `Src/Newtonsoft.Json/Newtonsoft.Json.csproj:3` sets the default `TargetFrameworks` to `net10.0;net8.0;net6.0;net45;net40;net35;net20;netstandard2.0`. The same file has per-framework property groups for these at lines 61–85.

**13.0.3 baseline:** I'm using my recollection of the 13.0.3 package's `lib/` folders: net6.0, net45, net40, net35, net20, netstandard2.0, netstandard1.3 and netstandard1.0. I did not check this against the package or the repo history in this session.

**Comparison:**
- **Added:** net8.0 and net10.0. Line 61 gives them a shared property group.
- **Dropped:** netstandard1.0 and netstandard1.3. They are not in the list at line 3, and the file has no property groups for them.
- **Unchanged:** net6.0, net45, net40, net35, net20 and netstandard2.0.

Line 4 lets `$(LibraryFrameworks)` override the list, so the targets above are only the defaults.