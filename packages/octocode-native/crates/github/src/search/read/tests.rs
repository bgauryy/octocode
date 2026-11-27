use super::*;

/// Every caller page of a walk lies whole inside the provider page it
/// reads, a second page asks for exactly itself, and no page asks for more
/// than everything the walk read before it.
#[test]
fn a_walk_reads_provider_pages_that_double() {
    for per_page in [1, 3, 7, 30, 50, 99] {
        for page in 2..=40 {
            let Some((provider_page, size)) = grown_page(page, per_page) else {
                assert!(per_page * (1 << (page - 1).ilog2()) > SEARCH_PAGE);
                continue;
            };
            let held = (provider_page - 1) * size..provider_page * size;
            let asked = (page - 1) * per_page..page * per_page;
            assert!(held.start <= asked.start && asked.end <= held.end);
            assert!(
                size <= asked.start,
                "page {page} asks for more than was read"
            );
            assert!(size <= SEARCH_PAGE);
        }
    }
    assert_eq!(grown_page(1, 5), None);
    assert_eq!(grown_page(2, 5), Some((2, 5)));
    assert_eq!(grown_page(3, 5), Some((2, 10)));
    assert_eq!(grown_page(4, 5), Some((2, 10)));
    assert_eq!(grown_page(5, 5), Some((2, 20)));
    assert_eq!(grown_page(2, 100), Some((2, 100)));
    assert_eq!(grown_page(3, 100), None);
}

/// A caller page inside the cap reads the whole provider pages that hold
/// it; one that runs past the cap reads GitHub's own page.
#[test]
fn a_page_past_the_cap_is_read_as_asked() {
    assert_eq!(provider_pages(1, 100), Some((1, 1)));
    assert_eq!(provider_pages(10, 100), Some((10, 10)));
    assert_eq!(provider_pages(111, 9), Some((10, 10)));
    assert_eq!(provider_pages(33, 30), Some((10, 10)));
    assert_eq!(provider_pages(34, 30), None);
    assert_eq!(provider_pages(112, 9), None);
    assert_eq!(provider_pages(11, 100), None);
}
