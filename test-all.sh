#!/bin/bash

set -e

for flags in "" "--release" "--features=debug-is-prod" "--no-default-features --features=debug-is-prod"; do
  cargo test -p reinda-core $flags
  cargo test -p reinda-macros $flags
  cargo test $flags
done
