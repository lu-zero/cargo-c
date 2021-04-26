# This is an ugly workaround.
#
# the github actions do not expand globs and somehow `grcov .` does not work as intended

FILES=`find . | grep profraw | xargs`

echo $FILES

ls -l $FILES
ls -l

echo "Trying grcov"
grcov $FILES --binary-path target/debug/deps/ -s . -t lcov --branch --ignore-not-existing --ignore '../**' --ignore '/*' -o coverage.lcov
