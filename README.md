# CosmWasm Staking Api
An [Abstract](https://abstract.money) api for staking tokens in CosmWasm contracts.

# Features
## Deployment
At the root of the project, wasm the contracts using:
### Wasming
```bash
cargo build
RUSTFLAGS='-C link-arg=-s' cargo wasm --package cw-staking
RUSTFLAGS='-C link-arg=-s' cargo wasm --package autocompounder
```
### Deploying
```shell
cargo deploy --package cw-staking
# UPLODAD AUTOCOMPOUONDER
cargo deploy --package autocompounder --code-id CODO_ID
```

### VAults
```shell
(cd scripts && cargo +nightly run --bin init_4t2_vault)
```
