# This is an ugly workaround.
#
# the github actions do not expand globs and somehow `grcov .` does not work as intended

ls -l
ls -l *.profraw
find . -name \*.profraw
grcov *.profraw --binary-path target/debug/deps/ -s . -t lcov --branch --ignore-not-existing --ignore '../**' --ignore '/*' -o coverage.lcov
