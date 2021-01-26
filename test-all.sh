#!/bin/bash

set -e

for flags in "" "--release" "--features=debug_is_prod" "--no-default-features --features=debug_is_prod"; do
  cargo test -p reinda-core $flags
  cargo test -p reinda-macros $flags
  cargo test $flags
done
