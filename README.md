# FORTYTWO CONTRACTS (DEPRICATED)

The development of cosmwasm smartcontracts within fortytwo has been depricated since **March 2024**. This codebase is not maintained and is only here for reference.


An [Abstract](https://abstract.money) api for staking tokens in CosmWasm contracts.

# Features


# Access to Abstract

In order to pull the abstract contracts for testing you need to enable http auth in github.
https://doc.rust-lang.org/cargo/appendix/git-authentication.html

If you're on mac add the following to your global git config (located at `~/.gitconfig`)

```none
[credential]
    helper = osxkeychain
```

## Deployment
At the root of the project, wasm the contracts using:
### Wasming
#### All
```bash
just wasm
```
#### Single Modules
```bash
just wasm-module <module> <args>
```

### Deploying
#### All
Wasmed automatically! Be sure to check the default-features!
```shell
just deploy <chain-id>
```
#### Individual
```shell
just deploy-module cw-staking <chain-id> <args>
```

### Vaults
```shell
just create-vault <chain-id> <paired-asset-id>
```

## NOTE
Cw-staking deployment for **Terra Testnet**:
```shell
just wasm-module cw-staking --features pisco-1 --no-default-features
```


### Update dependencies

For now, this repo depends on the `feature/impl-in-origin-repo` branch of the abstract adapters repo. This needs to be changed as this branch will move in the future