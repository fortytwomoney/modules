# contracts

## File Structure
- [`src`](src) - source code
  - [`contract.rs`](src/contract.rs) - contract implementation with the top-level handlers for `instantiate`, `query`, `execute`, `migrate`
  - [`handlers`](src/handlers) - contains the handlers for the app
    - [`instantiate.rs`](src/handlers/instantiate.rs) - contains the msg handlers for the `instantiate` entrypoint
    - [`query.rs`](src/handlers/query.rs) - contains the msg handlers for the `query` entrypoint
    - [`commands.rs`](src/handlers/execute.rs) - contains the msg handlers for the `execute` entrypoint
    - [`migrate.rs`](src/handlers/migrate.rs) - contains the msg handlers for the `migrate` entrypoint
    - [`reply.rs`](src/handlers/reply.rs) - contains the msg handlers for the `reply` entrypoint
  - [`state.rs`](src/package/state.rs) - contains the state of the contract
  - [`msg.rs`](src/package/msg.rs) - contains the messages and responses


## Vault tokens and rewards

$ V_{new} = \frac{V_{old}}{LP_{old}} * LP_{new} $


$V_{new}$ = Amount of vault tokens to be minted to the user
$V_{old}$ = Total amount of vault tokens currently minted
$LP_{new}$ = LP tokens minted by the user 
$LP_{old}$ = All staked Lp tokens currently in the vault (assuming all staked)
*This doesnt take into account the number of tokens that being unbonded*

