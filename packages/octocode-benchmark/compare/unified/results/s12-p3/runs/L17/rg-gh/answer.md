Newtonsoft.Json enforces required members after the whole JSON object has been read. It tracks which properties appeared while reading, then checks each one against its `Required` setting. All line numbers below are in `Src/Newtonsoft.Json/Serialization/`.

**1. Deciding whether to track.** `JsonObjectContract.HasRequiredOrDefaultValueProperties` (`JsonObjectContract.cs:143`) is true if any of these hold:
- The contract's `ItemRequired` is not `Default`.
- Any property's `Required` is not `Default`.
- Any property has `DefaultValueHandling.Populate`.

`PopulateObject` (`JsonSerializerInternalReader.cs:2467`) builds `propertiesPresence` only if that flag is true or the serializer's `DefaultValueHandling` has `Populate`. Every contract property starts as `PropertyPresence.None`. The constructor/creator path does the same with `trackPresence` at line 2065.

**2. Recording presence while reading.** `SetPropertyPresence` (`JsonSerializerInternalReader.cs:2723–2748`) sets a property's presence as the reader meets it. The value is `Null` for a JSON null or undefined token, or for an empty string that coerces to null. Otherwise it is `Value`. Properties that are ignored or skipped by `ShouldDeserialize` are still recorded (lines 2526 and 2543). Creator parameters record `None`, `Null` or `Value` at lines 2091–2121.

**3. Enforcing at the end of the object.** After the read loop finishes, line 2583 calls `EndProcessProperty` (defined at line 2671) for each tracked property. The creator path does the same at line 2280. It resolves the requirement as follows, using `JsonSerializerInternalReader.cs:2677`:
- `Required.Default` if the property is ignored.
- Otherwise the property's `_required`, then the contract's `ItemRequired`, then `Required.Default`.

It then throws a `JsonSerializationException` in these cases:
- **Presence `None`, with `Required.Always` or `Required.AllowNull`:** the message is "Required property '{0}' not found in JSON." (lines 2682–2685).
- **Presence `Null`, with `Required.Always`:** the message is "Required property '{0}' expects a value but got null." (lines 2701–2704).
- **Presence `Null`, with `Required.DisallowNull`:** the message is "Property '{0}' expects a non-null value." (lines 2705–2708).

For a missing property with no requirement, `DefaultValueHandling.Populate` can set the default value instead (lines 2688–2698).

**4. Error handling.** The check is wrapped in try/catch. If `IsErrorHandled` returns true (an error handler is in place), the error is swallowed and `HandleError` runs. Otherwise the exception is rethrown (lines 2711–2721).

**Not checked:** I did not look at how `Required` is populated from `[JsonProperty(Required=...)]`, `[JsonRequired]` or `[JsonObject(ItemRequired=...)]` in the contract resolver. I also did not check whether C# `required` keyword support exists at this commit.