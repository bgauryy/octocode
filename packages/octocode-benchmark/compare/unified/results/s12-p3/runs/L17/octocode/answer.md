Required members are enforced after the object's JSON has been read. Newtonsoft.Json records which properties appeared, then checks the ones that didn't. All of this is in `Src/Newtonsoft.Json/Serialization/JsonSerializerInternalReader.cs`. I didn't read how `HasRequiredOrDefaultValueProperties` is computed, or the code that sets each property's presence.

1. **Tracking is only switched on when needed.** `PopulateObject` builds a `Dictionary<JsonProperty, PropertyPresence>`, starting every property at `None`. It does this only if `contract.HasRequiredOrDefaultValueProperties` is true or `DefaultValueHandling.Populate` is set (`:2467-2469`). The constructor-based path also has this check (`:2065`).

2. **Presence is recorded while reading.** `SetPropertyPresence` (`:2726`) updates the dictionary as each property is read. I saw only its signature, not its body. I'm inferring it marks a property as having a value or as null.

3. **Checks run once the object is fully read.** `EndProcessProperty` is called for each tracked property (`:2588`, in the populate path with `setDefaultValue=true`). It's also called in the constructor path (`:2280`). It only acts when the presence is `None` (property absent) or `Null` (property present but null) (`:2673`).

4. **The effective setting.** `resolvedRequired` is `Required.Default` if the property is ignored. Otherwise it is `property._required ?? contract.ItemRequired ?? Required.Default` (`:2677`).

5. **What gets thrown.** All of these are a `JsonSerializationException`:
   - **Property absent, with `Required.Always` or `Required.AllowNull`:** `"Required property '{0}' not found in JSON."` (`:2682-2685`).
   - **Property null, with `Required.Always`:** `"Required property '{0}' expects a value but got null."` (`:2701-2704`).
   - **Property null, with `Required.DisallowNull`:** `"Property '{0}' expects a non-null value."` (`:2705-2708`).
   - **Property absent, not required:** if `DefaultValueHandling.Populate` applies and the property is writable, the default value is set instead (`:2687-2698`).

6. **Error handling.** The checks sit inside a try/catch. If `IsErrorHandled` returns true, for example through an `OnError` callback or the serializer's `Error` event, the error is swallowed and `HandleError` runs. Otherwise the exception is rethrown (`:2712-2721`).