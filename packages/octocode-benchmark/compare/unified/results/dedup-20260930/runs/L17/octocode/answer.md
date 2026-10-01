Required members are enforced after the JSON object has been read. The reader records which properties appeared, then checks the ones that didn't. All of this is in `Src/Newtonsoft.Json/Serialization/JsonSerializerInternalReader.cs` unless noted.

1. **Deciding whether to track.** `JsonObjectContract.HasRequiredOrDefaultValueProperties` (`JsonObjectContract.cs:143`) is true if `ItemRequired` is not `Default`. It is also true if any property has `Required != Default` or `DefaultValueHandling.Populate`.
2. **Starting state.** If that flag is true (or the serializer's default is `Populate`), `propertiesPresence` is a dictionary of every contract property mapped to `PropertyPresence.None` (`:2467-2469`). Otherwise it is null and nothing is tracked.
3. **Recording presence.** As each property is read, `SetPropertyPresence` (`:2526`, `:2543`, defined at `:2726-2748`) records what was found:
   - A JSON null or undefined becomes `Null`.
   - A string that coerces to null (empty string for a nullable type) also becomes `Null`.
   - Anything else becomes `Value`.
4. **Checking afterwards.** Once the object has been read, the code loops over `propertiesPresence` and calls `EndProcessProperty(..., presence, true)` for each entry (`:2581-2588`). The method is defined at `:2671`.
5. **Rules in `EndProcessProperty`.** Only `None` and `Null` are examined. The required level is `property.Ignored ? Default : property._required ?? contract.ItemRequired ?? Default` (`:2677`). The property-level setting wins over the contract's `ItemRequired`.
   - **Missing (`None`):** `Required.Always` or `Required.AllowNull` throws `JsonSerializationException` with "Required property '{0}' not found in JSON." (`:2682-2684`).
   - **Null (`Null`):**
     - `Required.Always` throws "Required property '{0}' expects a value but got null." (`:2701-2703`).
     - `Required.DisallowNull` throws "Property '{0}' expects a non-null value." (`:2705-2707`).
6. **Error handling.** The checks are inside a try/catch. If `IsErrorHandled` returns true (an error handler on the serializer or contract), the error is swallowed and `HandleError` runs. Otherwise the exception is rethrown (`:2712-2720`).
7. **Constructor-based creation.** `EndProcessProperty` is also called at `:2280`, on the path that appears to handle parameterized-constructor creation. I didn't read that block, so I can't say exactly how it passes presence there.