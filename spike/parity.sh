#!/bin/zsh
# SPIKE (#903): refresh the public recognition fixtures' snapshots under one configuration,
# in a scratch copy. usage: parity.sh <name> [env assignments...]
set -e
name=$1; shift
S=${PARITY_DIR:?}
dst=$S/$name
[[ -d $dst ]] && mv -f $dst $S/.old-$name-$$
mkdir -p $dst && cp -R tests/fixtures/recognition/en tests/fixtures/recognition/ru $dst/
start=$(date +%s)
env "$@" /usr/bin/time -l ${RECOGNIZE:-spike/target/release/recognize} --refresh-snapshots $dst > $S/$name.log 2>&1 || echo "exit $?" >> $S/$name.log
echo "$name: $(( $(date +%s) - start ))s" | tee -a $S/times.txt
grep -E "maximum resident|peak memory" $S/$name.log | tee -a $S/times.txt
