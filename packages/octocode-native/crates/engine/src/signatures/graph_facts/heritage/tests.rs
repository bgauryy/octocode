use super::base_type_name;

#[test]
fn base_type_names_drop_type_arguments_and_keep_paths() {
    assert_eq!(base_type_name("Base").as_deref(), Some("Base"));
    assert_eq!(base_type_name("List<String>").as_deref(), Some("List"));
    assert_eq!(
        base_type_name("ns::Base<T, U<V>>::Inner").as_deref(),
        Some("ns::Base::Inner")
    );
    assert_eq!(
        base_type_name("std::fmt::Display").as_deref(),
        Some("std::fmt::Display")
    );
    assert_eq!(base_type_name("pkg.Base").as_deref(), Some("pkg.Base"));
    assert_eq!(base_type_name("mixin(A)"), None);
    assert_eq!(base_type_name("Broken<T"), None);
    assert_eq!(
        base_type_name("a::r#Trait<T>").as_deref(),
        Some("a::r#Trait")
    );
    assert_eq!(base_type_name("a#b"), None);
    assert_eq!(base_type_name("r#"), None);
    assert_eq!(base_type_name(""), None);
}
