#[cfg(feature = "capi")]
mod capi {

    use inline_c::assert_c;

    #[test]
    fn test_capi() {
        (assert_c! {
        #include <example_project_with_subdir.h>

        int main() {
            if (example_project_with_subdir_add_two(40) != 42) {
                return 1;
            }
        }
            })
        .success();
    }
}
