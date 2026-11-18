//! GitHub search, structure and clone rows read from the serialized tool
//! result: links in a tree listing, follow-up leads, hints by error kind, and
//! what a row does not echo back.

mod leads_and_hints;
mod materialize_write;
mod path_rename;
mod refs;
mod search_facts;
mod symlink;
