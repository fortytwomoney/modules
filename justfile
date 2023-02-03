# https://cheatography.com/linux-china/cheat-sheets/justfile/

update:
  cargo update

# `just wasm-module cw-staking --features terra-testnet --no-default-features`
wasm-module module +args='':
  RUSTFLAGS='-C link-arg=-s' cargo wasm --package {{module}} {{args}}

# Deploy a module to the chain
###deploy-module module +args='': (wasm-module module)
# `just deploy-module autocompounder --network-id pisco-1`
deploy-module module network +args='':
  cargo deploy --package {{module}} -- --network-id {{network}} {{args}}

# wasm all the things!
wasm:
  RUSTFLAGS='-C link-arg=-s' cargo wasm --package cw-staking
  RUSTFLAGS='-C link-arg=-s' cargo wasm --package autocompounder

# would be really nice to be able to say "abstarct register autocompounder"
deploy network: wasm
  just deploy-module autocompounder {{network}}
  just deploy-module cw-staking {{network}}

create-vault network paired +args='':
  (cd scripts && cargo +nightly run --bin init_4t2_vault -- --network-id {{network}} --paired-asset {{paired}} {{args}})

build:
  cargo build

test:
  cargo nextest run

schema-module module version:
  #!/usr/bin/env bash
  set -euxo pipefail
  outdir="$(cd ../../Abstract/schemas && echo "$PWD")/4t2/{{module}}/{{version}}"
  cargo schema --package {{module}} && mkdir -p "$outdir"; cp -a "schema/." "$outdir";

publish-schemas version:
  just schema-module cw-staking {{version}}
  just schema-module autocompounder {{version}}


create-pisco-1-vaults +args='':
  just create-vault pisco-1 "'terra2>astro'" {{args}}

wasm-pisco-1:
  just wasm-module cw-staking --features terra-testnet --no-default-features
  just wasm-module autocompounder

deploy-pisco-1: wasm-pisco-1
  just deploy-module cw-staking pisco-1
  just deploy-module autocompounder pisco-1

full-deploy-pisco-1: deploy-pisco-1
  just create-vault pisco-1 "'terra2>astro'" --os-id 2
  just create-vault pisco-1 "'terra2>stb'" --os-id 3

# Use this to wasm and update the autocompounder code on pisco-1 while updating the cw-staking version as well
update-autocompounder-pisco-1 network='pisco-1':
  just wasm-module autocompounder
  just deploy-module autocompounder {{network}}
  just deploy-module cw-staking {{network}} --code-id 7374

update-cw-staking-pisco-1:
  just wasm-module cw-staking --features terra-testnet --no-default-features
  just deploy-module cw-staking pisco-1
  just deploy-module autocompounder pisco-1 --code-id 7372
#  just deploy pisco-1
