use super::{Imported, imported};

fn row<'a>(path: &[&'a str], name: &'a str, local: &'a str) -> Imported<'a> {
    Imported {
        path: path.to_vec(),
        name,
        local,
    }
}

#[test]
fn import_clauses_name_each_binding() {
    assert_eq!(
        imported(&[
            "cats.data.ContT",
            "cats.syntax.all._",
            "a.{B, C => D, E => _, given, _}",
            "x.y.*, p.{Q as R}",
        ]),
        [
            row(&["cats", "data"], "ContT", "ContT"),
            row(&["cats", "syntax", "all"], "_", "_"),
            row(&["a"], "B", "B"),
            row(&["a"], "C", "D"),
            row(&["a"], "_", "_"),
            row(&["x", "y"], "_", "_"),
            row(&["p"], "Q", "R"),
        ]
    );
}
