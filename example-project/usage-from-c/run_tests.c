#include <example_project/example_project.h>
#include <stdio.h>

int main() {
    ExampleProjectOddCounter *counter = example_project_oddcounter_new(4);
    if (counter) {
        printf("Unexpected success\n");
        return 1;
    }
    counter = example_project_oddcounter_new(5);
    if (!counter) {
        printf("Error creating ExampleProjectOddCounter\n");
        return 1;
    }
    example_project_oddcounter_increment(counter);
    uint32_t result = example_project_oddcounter_get_current(counter);
    example_project_oddcounter_free(counter);
    if (result == 7) {
        return 0;
    } else {
        printf("Error: unexpected result: %d\n", result);
        return 1;
    }
}
