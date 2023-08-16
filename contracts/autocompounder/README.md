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



## Redeem Vault Tokens Flow

When calling 'redeem' on the vault token, there are 2 cases that lead to a different flow:

1. If the staking contract has an unbonding period
2. If the staking contract does not have an unbonding period

The flow is shown below, where the dotted arrows represent time passing.

```Mermaid
graph TD

    A{{USER: Redeem Vault Token}}
    B{Unbonding Period}
    C[Register Pre Claim]
    D[Save or Update Claim]
    E[Unbond Claims]
    G[Unstake LP Tokens]
    H[Burn Vault Tokens]

    I{{USER: Withdraw Claims}}
    J[Claim Unbonded Tokens]
    K[Reply LP Withdrawal]
    L[Swap LP Tokens]
    X[Send to User]

    M[Redeem Without Bonding Period]
    N[Unstake LP Tokens]
    O[Burn Vault Tokens]
    P[Withdraw LP Tokens]
    Q[Swap LP Tokens]
    R[Send to User]


    S{{BOT: Batch Unbond}}

    A{{USER: Redeem Vault Tokens}} -->| User | B
    B{Unbonding Period?} -->|Yes| C

    C --> D
    D -. unbonding period passes...-> I
    S--> E
    E --> G
    G --> H
    H -. unbonding period  ...-J
    I ==> J
    J --> K
    K --> L
    L --> X

    B -->|No| M
    M --> N
    N --> O
    O --> P
    P --> Q
    Q --> R
```
```Mermaid
graph TD

    A{{USER: Redeem Vault Token}}
    B{Unbonding Period}
    C[Register Pre Claim]
    D[Save or Update Claim]
    E[Unbond Claims]
    G[Unstake LP Tokens]
    H[Burn Vault Tokens]

    I{{USER: Withdraw Claims}}
    J[Claim Unbonded Tokens]
    K[Reply LP Withdrawal]
    L[Swap LP Tokens]
    X[Send to User]




    S{{BOT: Batch Unbond}}

    A{{USER: Redeem Vault Tokens}} -->| User | B
    B{Unbonding Period?} -->|Yes| C

    C --> D
    D -. unbonding period passes...- I
    S--> E
    E --> G
    G --> H
    I ==> J
    J --> K
    K --> L
    L --> X

```
