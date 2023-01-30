wasm-modules:
  RUSTFLAGS='-C link-arg=-s' cargo wasm --package cw-staking
  RUSTFLAGS='-C link-arg=-s' cargo wasm --package autocompounder

deploy-modules:
  cargo deploy --package autocompounder
  cargo deploy --package cw-staking

create-vault:
  (cd scripts && cargo +nightly run --bin init_4t2_vault --  --paired-asset pood)

build:
  cargo build

test:
  cargo nextest run
