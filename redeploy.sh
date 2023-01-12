#!/usr/bin/env bash
set -o errexit -o nounset -o pipefail
command -v shellcheck >/dev/null && shellcheck "$0"

RUSTFLAGS='-C link-arg=-s' cargo wasm --package cw-staking
RUSTFLAGS='-C link-arg=-s' cargo wasm --package autocompounder

cargo deploy --package cw-staking
