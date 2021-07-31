#include <example_project_with_subdir/example_project_with_subdir.h>
#include <stdio.h>

int main() {
    if (example_project_with_subdir_add_two(40) != 42) {
        printf("Error: unexpected result");
        return 1;
    }
}
