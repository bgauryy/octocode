use super::*;
use reqwest::header::HeaderValue;

#[test]
fn pagination_reads_every_link_header_and_relation_token() {
    let mut headers = HeaderMap::new();
    headers.append(
        "link",
        HeaderValue::from_static("<https://api.github.com/items?page=1>; rel=\"prev\""),
    );
    headers.append(
        "link",
        HeaderValue::from_static("<https://api.github.com/items?page=3>; rel=\"next\""),
    );
    headers.append(
        "link",
        HeaderValue::from_static("<https://api.github.com/items?page=5>; rel=\"last\""),
    );
    assert_eq!(
        parse_next(&headers).unwrap().unwrap().as_str(),
        "https://api.github.com/items?page=3"
    );
    assert_eq!(last_page(&headers), Some(5));
    headers.clear();
    headers.append("link", HeaderValue::from_static("<https://api.github.com/items?page=3>; rel=\"prev next\"; title=\"a,b\", <https://api.github.com/items?page=5>; rel=last"));
    assert_eq!(
        parse_next(&headers).unwrap().unwrap().as_str(),
        "https://api.github.com/items?page=3"
    );
    assert_eq!(last_page(&headers), Some(5));
}
