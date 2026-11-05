//! What a window's continuation pins: the digest of the rows listed so far, and
//! what a walk that continues checks against it. Rows alone decide: when the
//! entries a window passes list what the windows before it listed, what comes
//! after them lists every other row exactly once, whatever else moved around them.
use super::super::files::is_traversal;
use super::super::listing::Listing;
use super::super::rows::{self, Lines, Ranking};
use octocode_engine::digest::HashWriter;
use octocode_engine::types::FileSystemEntry;
use serde_json::json;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

/// The digest a window's next one pins: the listing's shape, then each of its
/// entries in listing order as its rows show them.
#[derive(Clone)]
pub(in crate::tools::structure_search) struct ListingDigest(HashWriter);

impl ListingDigest {
    pub(in crate::tools::structure_search) fn new<Q: Listing>(q: &Q, root: &Path) -> Self {
        let mut writer = HashWriter::default();
        let shape = json!([q.walk_identity(root), q.view(), q.sort(), Q::LAYOUT]);
        // A plain value into a hash: neither side can fail.
        let _ = serde_json::to_writer(&mut writer, &shape);
        Self(writer)
    }

    pub(in crate::tools::structure_search) fn hex(&self) -> String {
        hex::encode(self.0.clone().finalize())
    }

    /// The reader of what a replay lists, hashed into `digest` as the windows
    /// before it hashed those rows.
    pub(in crate::tools::structure_search) fn replayed(
        digest: &Arc<Mutex<Self>>,
    ) -> octocode_engine::types::ReplayedEntries {
        let digest = Arc::clone(digest);
        octocode_engine::types::ReplayedEntries(Arc::new(move |entry| {
            let mut digest = digest.lock().unwrap_or_else(PoisonError::into_inner);
            rows::digest_entry(&mut digest.0, entry, None);
        }))
    }
}

/// The snapshot `q` continues its listing under: a window past the first or a
/// resume, on its first page. A later page's snapshot pins that page's own cut.
pub(in crate::tools::structure_search) fn pinned<Q: Listing>(q: &Q) -> Option<&str> {
    q.snapshot()
        .filter(|_| q.page() == 1 && (q.scan_offset() > 0 || q.walk_offset() > 0))
}

/// What a walked window's digests pin.
#[derive(Default)]
pub(in crate::tools::structure_search) struct Pins {
    /// What the windows before this one listed: the query's snapshot, when
    /// the walk is the same.
    pub(in crate::tools::structure_search) before: Option<String>,
    /// The listing through this window: what its pages and next window pin.
    pub(in crate::tools::structure_search) through: Option<ListingDigest>,
    /// A walk cut before the rows its query pinned hands that pin on: the
    /// resume reaches them.
    owed: Option<String>,
}

impl Pins {
    /// What this window's pages and next window pin.
    pub(in crate::tools::structure_search) fn pin_through(&self) -> Option<String> {
        self.owed
            .clone()
            .or_else(|| self.through.as_ref().map(ListingDigest::hex))
    }
}

/// How the walk [`pins`] hashes ran.
pub(in crate::tools::structure_search) struct PinnedWalk {
    /// The rows it passed: a resume's, or the rows through a kept cursor.
    pub(in crate::tools::structure_search) passed: Option<ListingDigest>,
    /// `passed` is where its pin's window stands ([`super::Carried`]).
    pub(in crate::tools::structure_search) carried: bool,
    /// A listing walk; a probe pins nothing.
    pub(in crate::tools::structure_search) listing: bool,
    pub(in crate::tools::structure_search) time_cut: bool,
    /// A window follows this one by row count.
    pub(in crate::tools::structure_search) continues: bool,
}

/// Hash `ranking`'s entries for a listing of more than one window: resumed,
/// past its first (`scanOffset`), or with one to come (`continues`, or a time
/// cut). A walk cut by time hashes its rows in walk order, what its resume can
/// replay and with no line count (a stamp shows what would change one); a sorted
/// one past `scanOffset` lists nothing, so pins nothing.
pub(in crate::tools::structure_search) fn pins<Q: Listing>(
    q: &Q,
    root: &Path,
    (entries, lines, ranking): (&[FileSystemEntry], &[Option<Lines>], &Ranking),
    walk: PinnedWalk,
) -> Pins {
    let PinnedWalk {
        passed,
        carried,
        listing,
        time_cut,
        continues,
    } = walk;
    let traversal = is_traversal(&q.sort());
    let whole_window = passed.is_none() && !time_cut && q.scan_offset() == 0 && !continues;
    let lists_nothing = time_cut && !traversal && q.scan_offset() > 0;
    if !listing || whole_window || lists_nothing {
        return Pins::default();
    }
    let mut digest = passed.unwrap_or_else(|| ListingDigest::new(q, root));
    let mut walk_order;
    let order = if time_cut {
        walk_order = ranking.order().to_vec();
        walk_order.sort_unstable();
        walk_order.as_slice()
    } else {
        ranking.order()
    };
    // Rows reached in walk order are the ones listed first; a sorted window
    // pins all of its rows.
    let listed =
        (!carried && (traversal || time_cut || q.scan_offset() == 0)).then_some(q.scan_offset());
    let mut at_listed = None;
    for (at, &entry) in order.iter().enumerate() {
        if listed == Some(at) {
            at_listed = Some(digest.hex());
        }
        let lines = lines.get(entry).copied().flatten().filter(|_| !time_cut);
        rows::digest_entry(&mut digest.0, &entries[entry], lines);
    }
    if listed == Some(order.len()) {
        at_listed = Some(digest.hex());
    }
    let mut pins = Pins::default();
    if let Some(pin) = pinned(q) {
        // A listing that walks its whole scope pinned its whole scope, unless
        // the pin is a resume's: only the rows through the end can settle that.
        let settled = if q.early_exit() || !traversal {
            at_listed
        } else {
            at_listed.filter(|at_listed| at_listed == pin)
        };
        match settled {
            _ if carried => pins.before = Some(pin.to_owned()),
            Some(settled) => pins.before = Some(settled),
            None if time_cut => pins.owed = Some(pin.to_owned()),
            None => pins.before = Some(digest.hex()),
        }
    }
    pins.through = Some(digest);
    pins
}
