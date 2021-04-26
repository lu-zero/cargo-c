# This is an ugly workaround.
#
# the github actions do not expand globs and somehow `grcov .` does not work as intended

MY_GLOB=`ls -l && ls -l *.profraw`
echo "Trying the glob ${MY_GLOB}"

echo "Trying the find `find . -name \*.profraw`"

echo "Trying grcov"
grcov *.profraw --binary-path target/debug/deps/ -s . -t lcov --branch --ignore-not-existing --ignore '../**' --ignore '/*' -o coverage.lcov
echo "Trying grcov ."
grcov . --binary-path target/debug/deps/ -s . -t lcov --branch --ignore-not-existing --ignore '../**' --ignore '/*' -o coverage.lcov
