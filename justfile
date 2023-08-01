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
  cargo clippy --fix --allow-staged --allow-dirty --all-features
  cargo fmt --all

create-vault network paired other_asset +args='':
  (cd scripts && cargo +nightly run --bin init_4t2_vault -- --network-id {{network}} --paired-asset {{paired}} --other-asset {{other_asset}} {{args}})

create-vault-acc network paired other_asset account +args='':
  (cd scripts && cargo +nightly run --bin init_4t2_vault -- --network-id {{network}} --paired-asset {{paired}} --other-asset {{other_asset}} --account-id {{account}} {{args}})

deposit-vault network vault-id:
  (cd scripts && cargo +nightly run --bin test_compound -- --network-id {{network}} --vault-id {{vault-id}})

build:
  cargo build

test:
  cargo nextest run --all-features

schema-module module version:
  #!/usr/bin/env bash
  set -euxo pipefail
  outdir="$(cd ../../Abstract/schemas && echo "$PWD")/4t2/{{module}}/{{version}}"
  cargo schema --package {{module}} && mkdir -p "$outdir"; cp -a "schema/." "$outdir";

publish-schemas version:
  just schema-module autocompounder {{version}}

create-pisco-1-vaults +args='':
  just create-vault pisco-1 "'terra2>astro'" {{args}}

# `just wasm-contract autocompounder--features export,terra --no-default-features`
wasm-contract module +args='':
  RUSTFLAGS='-C link-arg=-s' cargo wasm --package {{module}} {{args}}

# Wasm all the contracts in the repository for the given chain
wasm chain_name:
  just wasm-contract autocompounder --features export --no-default-features

# Deploy a module to the chain
# ??? deploy-module module +args='': (wasm-module module)
# `just deploy-module autocompounder pisco-1`
deploy-contract module network +args='':
  cargo deploy --package {{module}} -- --network-id {{network}} {{args}}

# Deploy all the apps
# would be really nice to be able to say "abstarct register autocompounder"
deploy network +args='':
  just wasm-contract autocompounder
  just deploy-contract autocompounder {{network}}


create-fee-collector network fee_asset commission_addr:
  (cd scripts && cargo +nightly run --bin init_4t2_vault -- --network-id {{network}} --fee-asset {{fee_asset}} --commission-addr {{commission_addr}}) 