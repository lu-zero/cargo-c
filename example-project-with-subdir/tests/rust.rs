use example_project_with_subdir::add_two;

#[test]
fn test() {
    assert_eq!(add_two(40), 42);
}
