project('example', 'c')

dep = dependency('example_project', static: true, required: true)

exe = executable('run_tests', files('run_tests.c'), dependencies: dep)

test('run_tests', exe)
