#!/usr/bin/env bash
set -o errexit -o nounset -o pipefail
command -v shellcheck >/dev/null && shellcheck "$0"

#cargo build
#
#RUSTFLAGS='-C link-arg=-s' cargo wasm --package autocompounder
#
#RUSTFLAGS='-C link-arg=-s' cargo wasm --package cw-staking

cargo deploy --package autocompounder -- --code-id 4070
cargo deploy --package cw-staking -- --prev-version 0.1.8

(cd scripts && cargo +nightly run --bin init_4t2_vault)
