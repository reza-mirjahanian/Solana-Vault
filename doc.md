# Reza Vault: A Simple Solana Token Vault


## 1. What I Implemented

I built a Solana program that acts as a basic vault for a single SPL token, referred to as "Asset A." Here's the core functionality:

- **Initialization**: An admin initializes the vault by creating a share mint (with the same decimals as Asset A) and setting up PDA-derived accounts for the vault state, authority, and asset storage. 

- **Deposits**: Users deposit Asset A tokens into the vault. The program calculates and mints proportional shares:
    - For the first deposit: 1:1 ratio (e.g., 100 tokens → 100 shares).
    - Subsequent deposits maintain the ratio using the formula:  
      $$ shares = \frac{amount \times total\_shares}{total\_asset} $$
    - Tokens are transferred to a PDA-owned account for security.

- **Withdrawals**: Users burn shares to withdraw Asset A proportionally:  
  $$ asset\_amount = \frac{shares \times total\_asset}{total\_shares} $$
    - Ensures fair accounting and updates global totals atomically.

- **Tracking**: Global totals (`total_asset` and `total_shares`) are stored in a `VaultState` account. Individual shares are tracked via SPL token balances in users' associated token accounts .

- **Events**: Emits `DepositEvent` and `WithdrawEvent` with details like user, amounts, and timestamp for off-chain monitoring.

- **Admin Controls**:
    - Emergency pause/unpause to halt deposits/withdrawals.
    - Access restricted to the initialized admin.

- **Security Features**:
    - Input validation (e.g., non-zero amounts, correct accounts).
    - Safe math with overflow checks (using `checked_add`/`checked_sub` and u128 for ratios).
    - PDA authority for minting/burning/transferring to prevent unauthorized actions.

- **Unit Tests**: Comprehensive tests in TypeScript using Anchor's framework, covering deposits, withdrawals, pauses, invalid inputs, and event emissions. Verified balances, ratios, and rejections.

This implementation uses "Anchor 0.31.1": . 

## 2. Assumptions Made

- **Single Token Focus**: The vault supports only one Asset A token (any SPL token can be used for testing, but it's fixed at initialization).
- **No Fees or Yield**: Assumes a simple share-based vault without performance fees, entry/exit fees, or interest accrual. Shares represent direct proportional ownership.
- **Decimal Matching**: Share mint uses the same decimals as Asset A for simplicity (e.g., 6 decimals).
- **Admin Trust**: The admin is trusted; no multi-sig or governance assumed.
- **Rounding Behavior**: Downward rounding in calculations (e.g., due to integer division); assumes users accept potential dust loss.
- **Testing Environment**: Tests assume a local Solana validator with airdropped SOL for fees and use a single user for simplicity.
- **No Rebase or Complex Logic**: Shares don't rebase; vault doesn't handle external yields or multiple assets.



## 3. Limitations and Known Issues

- **Rounding and Dust**: Proportional calculations can result in zero amounts due to integer division (handled with `RoundingError`), but small dust might be left in the vault over time. No mechanism to sweep or donate dust.
- **Single User in Tests**: Tests primarily use one user; multi-user interactions (e.g., concurrent deposits) aren't explicitly tested, though the logic is atomic.
- **Event Decoding in Tests**: Logs are parsed for event presence, but full decoding (e.g., asserting exact amounts) is commented out—needs proper Anchor event decoding for completeness.
- **No Advanced Features**: Lacks fees, slippage protection, multi-token support, or admin key rotation. .
- **Pause Scope**: Pause affects all users but doesn't handle in-flight transactions or provide a "drain" function for emergencies.
- **Security Audits**: Not audited; potential unknown vulnerabilities in edge cases (e.g., u64 overflows on massive deposits, though checked).



## 4. Next Steps (Optional)

### What I’d Do Next with More Time
- **Easy run method**: Docker compose file or something like that!
- **Frontend/UI**: Building a user-friendly dApp interface, as my focus was on the backend contract.
- **Enhance Features**: Add fees (e.g., performance fees on yields), multi-asset support, or integration with lending protocols.
- **Improved Testing**: Add fuzz testing for rounding edges, multi-user scenarios, and simulation of high-load conditions. Implement full event decoding in tests.
- **Security Upgrades**: Introduce governance (e.g., via DAO) for admin actions, add timelocks for pauses, and implement a "rage quit" for full vault drain.
- **Optimization**: Reduce compute by optimizing math (e.g., avoid u128 if possible) and add on-chain views for share price queries.
- **Deployment**: Write deployment scripts, integrate with frontends (e.g., React app), and add monitoring for events via webhooks.
- https://github.com/LiteSVM/litesvm/tree/master/crates/node-litesvm

### Any Areas Where I’d Need Support
- **Audits and Reviews**: External security audit from firms like OtterSec or Kudelski to catch subtle issues.
- **Advanced Solana Tools and testing**: Guidance on integrating with programs like Serum or Raydium for yields, or using Switchboard for oracles if adding price feeds.
