# CosmWasm Staking Api
An [Abstract](https://abstract.money) api for staking tokens in CosmWasm contracts.

# Features


# Access to Abstract

In order to pull the abstract contracts for testing you need to enable http auth in github. 
https://doc.rust-lang.org/cargo/appendix/git-authentication.html

If you're on mac add the following to your global git config (located at `~/.gitconfig`)

```none
[credential]
    helper = osxkeychain
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
