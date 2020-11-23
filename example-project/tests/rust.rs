use example_project::OddCounter;

#[test]
#[should_panic]
fn test() {
    OddCounter::new(40).unwrap();
}
