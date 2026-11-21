//! The path a call with `responseChars` takes: the pager plans the rows
//! with the page's envelope and pagination block counted, so a part worth
//! its fixed cost in a bare row plan must be worth it here too.
use super::*;
use crate::response::row_pages::tests::moderate_text::details_row;

/// A details row whose fields stated once, advisories without their text
/// and texts each fit a page: no page outgrows `responseChars` by more than
/// the fixed parts a page carries (its envelope, pagination block and the
/// context a part repeats: a later page without its advisories), and every
/// advisory arrives once, in order.
#[test]
fn a_page_outgrows_response_chars_by_no_more_than_a_pages_fixed_parts() {
    let data = details_row();
    let input = json!({"results":[{"index":0,"data":data}]});
    let chars = |value: &Value| value.to_string().encode_utf16().count();
    for budget in [1_500, 2_000, 3_000] {
        let pages = walk_tool("securityCheck", &input, budget);
        let advisories = |page: &Value| page["results"][0]["data"]["advisories"].clone();
        let fixed = pages[1..]
            .iter()
            .map(|page| chars(page) - chars(&advisories(page)))
            .max()
            .expect("the row is split");
        for (at, page) in pages.iter().enumerate() {
            assert!(
                chars(page) <= budget + fixed,
                "responseChars {budget}: page {} of {} holds {} chars, {fixed} of them fixed: {page}",
                at + 1,
                pages.len(),
                chars(page)
            );
        }
        // A part that starts an advisory names it; one that continues it
        // says so (`rowPart.continues`).
        let started = pages
            .iter()
            .filter(|page| page["results"][0]["rowPart"].get("continues").is_none())
            .flat_map(|page| advisories(page).as_array().cloned().unwrap_or_default())
            .map(|advisory| advisory["id"].clone())
            .collect::<Vec<_>>();
        let expected = data["advisories"]
            .as_array()
            .expect("advisories")
            .iter()
            .map(|advisory| advisory["id"].clone())
            .collect::<Vec<_>>();
        assert_eq!(started, expected, "responseChars {budget}");
    }
}
