# https://cheatography.com/linux-china/cheat-sheets/justfile/

update:
  cargo update

refresh:
  cargo clean
  cargo update

watch:
  cargo watch -x lcheck

check:
  cargo check --all-features

lintfix:
  cargo clippy --fix --allow-staged --allow-dirty 
  cargo fmt --all

create-vault network paired other_asset +args='':
  cargo +nightly run --bin init_4t2_vault -- --network-id {{network}} --paired-asset {{paired}} --other-asset {{other_asset}} {{args}}

deposit-vault network vault-id:
  cargo +nightly run --bin test_compound -- --network-id {{network}} --vault-id {{vault-id}}

build:
  cargo build

test:
  cargo nextest run --all-features

schema-module module version:
  #!/usr/bin/env bash
  set -euxo pipefail
  outdir="$(cd ../../Abstract/schemas && echo "$PWD")/4t2/{{module}}/{{version}}"
  cargo schema --package {{module}} && mkdir -p "$outdir"; cp -a "schema/." "$outdir";

# requires cargo-workspaces crate installed.
publish-schemas version:
  SCHEMA_OUT_DIR=$(cd ../../Abstract/schemas && echo "$PWD") \
  VERSION={{version}} \
  cargo ws exec --no-bail bash -lc 'cargo schema && { outdir="$SCHEMA_OUT_DIR/4t2/${PWD##*/}/$VERSION"; mkdir -p "$outdir"; rm -rf "schema/raw"; cp -a "schema/." "$outdir"; }'


create-pisco-1-vaults +args='':
  just create-vault pisco-1 "'terra2>astro'" {{args}}

# `just wasm-contract autocompounder--features export,terra --no-default-features`
wasm-contract module +args='':
  RUSTFLAGS='-C link-arg=-s -C target-feature=+sign-ext' cargo wasm --package {{module}} {{args}}

# Wasm all the contracts in the repository for the given chain
wasm:
  just wasm-contract autocompounder --features export --no-default-features
  mkdir -p artifacts
  cp target/wasm32-unknown-unknown/release/autocompounder.wasm artifacts

wasm-ac:
  just wasm-contract autocompounder --features export --no-default-features
  mkdir -p artifacts
  cp target/wasm32-unknown-unknown/release/autocompounder.wasm artifacts 


# Deploy a module to the chain
# ??? deploy-module module +args='': (wasm-module module)
# `just deploy-module autocompounder pisco-1`
deploy-contract module network +args='':
  cargo deploy --package {{module}} -- --network-id {{network}} {{args}}

# Deploy all the apps
# would be really nice to be able to say "abstarct register autocompounder"
deploy network +args='':
  just wasm
  just deploy-contract autocompounder {{network}}

migrate-vault network account-id +args='':
  cargo +nightly run --bin migrate_vault -- --network-id {{network}} --account-id {{account-id}} {{args}}

create-fee-collector network fee_asset commission_addr +args='':
  cargo +nightly run --bin init_fee_collector -- --network-id {{network}} --fee-asset {{fee_asset}} --commission-addr {{commission_addr}} {{args}}

upload-cw20-base network:
  cargo +nightly run --bin upload_cw20_base -- --network-id {{network}}