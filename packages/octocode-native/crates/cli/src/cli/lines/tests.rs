use super::*;
use serde_json::json;

fn sample() -> Value {
    json!({"results":[{"index":0,"rowPart":{"part":1,"of":2},"data":{
            "matchCount":2,"matchedLineCount":2,
            "next":{"nextPage":{"kind":"continue","tool":"localSearch","input":{"cursor":"issued-tool-page"}}},
            "files":[{"path":"a.rs","matches":[
                {"line":26,"value":"25\tuse a;\n26\tuse SearcherBuilder;\n27\tuse b;"},
                {"line":997,"value":"996\t) {\n997\t  let sb = SearcherBuilder::new();",
                 "enclosing":{"symbolName":"build","kind":"function","line":992,"endLine":1016}}]}]}}],
            "warnings":["incomplete"],
            "responsePagination":{"hasMore":true,"next":{"kind":"continue","tool":"localSearch","input":{"cursor":"issued-response-page"}}},
            "root":"/nowhere"})
}

fn context() -> Value {
    json!({"queries":[{"path":"s","matchString":["x"],"contextLines":{"before":1,"after":1}}]})
}

mod call_notes;
mod hit_lines;
mod listing_lines;
