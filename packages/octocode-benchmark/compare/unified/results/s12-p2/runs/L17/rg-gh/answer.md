Required members are enforced after the JSON object has been read. Presence is tracked per property while reading. Then each property's presence is checked against its resolved `Required` setting, and a `JsonSerializationException` is thrown on a violation. All paths are in `Src/Newtonsoft.Json/`, and I read this code without running it.

**Required setting** (`Required.cs`)
- `Default`: the property is not required.
- `AllowNull`: the property must be present in the JSON, but its value may be null.
- `Always`: the property must be present and must not be null.
- `DisallowNull`: the property is optional, but if present it must not be null.

**Tracking presence** (`Serialization/JsonSerializerInternalReader.cs`)
- `PopulateObject` builds a `propertiesPresence` dictionary only when `contract.HasRequiredOrDefaultValueProperties` is true or `DefaultValueHandling.Populate` is set (`:2467-2469`). Every property starts as `PropertyPresence.None`.
- As each JSON property is read, `SetPropertyPresence` (`:2726` onward, called at `:2526` and `:2543`) marks it `Value` or `Null`.
- The creator-based path tracks the same thing through `CreatorPropertyContext.Presence` (`:2048`, `:2108-2121`). It calls `EndProcessProperty` at `:2280`.

**Enforcement** (`EndProcessProperty`, `:2671-2720`)
- After the object is read, the loop at `:2583-2588` calls `EndProcessProperty` for each property. The check only runs when presence is `None` or `Null` (`:2673`).
- The effective requirement is resolved at `:2677` as `property.Ignored ? Default : property._required ?? contract.ItemRequired ?? Default`. A per-property setting therefore overrides the class-level `ItemRequired`.
- Presence `None` (property missing from the JSON) with `AllowNull` or `Always` throws `"Required property '{0}' not found in JSON."` (`:2682-2685`).
- Presence `Null` with `Always` throws `"Required property '{0}' expects a value but got null."` (`:2701-2704`).
- Presence `Null` with `DisallowNull` throws `"Property '{0}' expects a non-null value."` (`:2705-2708`).
- The thrown exception goes through `IsErrorHandled` (`:2712`). If an error handler handles it, deserialization continues. Otherwise it is rethrown.

**Not checked:** I did not read where `HasRequiredOrDefaultValueProperties` is computed (`Serialization/JsonObjectContract.cs:143`). I also did not check how the attributes or `ItemRequired` get mapped onto `_required`.