use anchor_lang::prelude::*;
use anchor_spl::token::{self, Burn, Mint, MintTo, Token, TokenAccount, Transfer};

declare_id!("Ht6zRm9hg3ebBpGeYGrgosdq7qQVNa6qQsmt3S7gdrv6");

#[program]
pub mod reza_vault {
    use super::*;

    // ─────────────────────────────────────────
    // INITIALISE ─ one-time call by admin
    // ─────────────────────────────────────────
    pub fn initialize_vault(
        ctx: Context<InitializeVault>,
        _vault_bump: u8,
        _auth_bump: u8,
    ) -> Result<()> {
        let state = &mut ctx.accounts.vault_state;
        state.asset_mint = ctx.accounts.asset_mint.key();
        state.share_mint = ctx.accounts.share_mint.key();
        state.vault_authority = ctx.accounts.vault_authority.key();
        state.vault_asset_account = ctx.accounts.vault_asset_account.key();
        state.admin = ctx.accounts.admin.key();
        state.total_asset = 0;
        state.total_shares = 0;
        state.paused = false;
        Ok(())
    }

    // ─────────────────────────────────────────
    // DEPOSIT ASSET A  ➜ mint vault shares
    // ─────────────────────────────────────────
    pub fn deposit_asset_a(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        // ╭──────────────────────────────────╮
        // │  Safety & vault-state sanity     │
        // ╰──────────────────────────────────╯
        require!(amount > 0, VaultError::InvalidAmount);
        require!(!ctx.accounts.vault_state.paused, VaultError::VaultPaused);

        let state = &mut ctx.accounts.vault_state;

        // ╭──────────────────────────────────╮
        // │  Transfer Asset A into vault     │
        // ╰──────────────────────────────────╯
        let cpi_accounts = Transfer {
            from: ctx.accounts.user_asset_account.to_account_info(),
            to: ctx.accounts.vault_asset_account.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        };
        token::transfer(
            CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts),
            amount,
        )?;

        // ╭──────────────────────────────────╮
        // │  Calculate shares to mint        │
        // ╰──────────────────────────────────╯
        let shares_to_mint: u64 = if state.total_shares == 0 || state.total_asset == 0 {
            // first deposit ⇒ 1:1 share ratio
            amount
        } else {
            // shares = amount * total_shares / total_asset   (all in u128 for safety)
            let shares = (amount as u128)
                .checked_mul(state.total_shares as u128)
                .unwrap()
                .checked_div(state.total_asset as u128)
                .unwrap();
            shares as u64
        };
        require!(shares_to_mint > 0, VaultError::RoundingError);

        // ╭──────────────────────────────────╮
        // │  Mint shares to the user         │
        // ╰──────────────────────────────────╯
        let state_key = state.key(); // own the Pubkey for the rest of the function
        let vault_authority_seeds: &[&[u8]] = &[
            b"vault_authority",
            state_key.as_ref(),
            &[ctx.bumps.vault_authority],
        ];
        let signer_seeds: &[&[&[u8]]] = &[vault_authority_seeds]; // &[&[u8]] → &[&[&[u8]]]

        let cpi_accounts = MintTo {
            mint: ctx.accounts.share_mint.to_account_info(),
            to: ctx.accounts.user_share_account.to_account_info(),
            authority: ctx.accounts.vault_authority.to_account_info(),
        };
        token::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                cpi_accounts,
                signer_seeds,
            ),
            shares_to_mint,
        )?;

        // ╭──────────────────────────────────╮
        // │  Update state                    │
        // ╰──────────────────────────────────╯
        state.total_asset = state
            .total_asset
            .checked_add(amount)
            .ok_or(VaultError::Overflow)?;
        state.total_shares = state
            .total_shares
            .checked_add(shares_to_mint)
            .ok_or(VaultError::Overflow)?;

        // ╭──────────────────────────────────╮
        // │  Emit event                      │
        // ╰──────────────────────────────────╯
        emit!(DepositEvent {
            user: ctx.accounts.user.key(),
            asset_amount: amount,
            shares_minted: shares_to_mint,
            timestamp: Clock::get()?.unix_timestamp,
        });
        msg!("DepositEvent finished!");
        Ok(())
    }

    // ─────────────────────────────────────────
    // WITHDRAW ASSET A  ➜ burn vault shares
    // ─────────────────────────────────────────
    pub fn withdraw_asset_a(ctx: Context<Withdraw>, shares: u64) -> Result<()> {
        require!(shares > 0, VaultError::InvalidAmount);
        require!(!ctx.accounts.vault_state.paused, VaultError::VaultPaused);

        let state = &mut ctx.accounts.vault_state;
        require!(shares <= state.total_shares, VaultError::InvalidShares);

        // ╭──────────────────────────────────╮
        // │  Burn shares from user           │
        // ╰──────────────────────────────────╯
        let cpi_accs = Burn {
            mint: ctx.accounts.share_mint.to_account_info(),
            from: ctx.accounts.user_share_account.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        };
        token::burn(
            CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accs),
            shares,
        )?;

        // ╭──────────────────────────────────╮
        // │  Calculate amount to send back   │
        // ╰──────────────────────────────────╯
        let asset_amount: u64 = (shares as u128)
            .checked_mul(state.total_asset as u128)
            .unwrap()
            .checked_div(state.total_shares as u128)
            .unwrap() as u64;

        require!(asset_amount > 0, VaultError::RoundingError);

        // ╭──────────────────────────────────╮
        // │  Transfer Asset A to user        │
        // ╰──────────────────────────────────╯
        let state_key = state.key(); // own the Pubkey for the rest of the function
        let vault_authority_seeds: &[&[u8]] = &[
            b"vault_authority",
            state_key.as_ref(),
            &[ctx.bumps.vault_authority],
        ];
        let signer_seeds: &[&[&[u8]]] = &[vault_authority_seeds]; // &[&[u8]] → &[&[&[u8]]]

        let cpi_accounts = Transfer {
            from: ctx.accounts.vault_asset_account.to_account_info(),
            to: ctx.accounts.user_asset_account.to_account_info(),
            authority: ctx.accounts.vault_authority.to_account_info(),
        };
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                cpi_accounts,
                signer_seeds,
            ),
            asset_amount,
        )?;

        // ╭──────────────────────────────────╮
        // │  Update state                    │
        // ╰──────────────────────────────────╯
        state.total_asset = state
            .total_asset
            .checked_sub(asset_amount)
            .ok_or(VaultError::Overflow)?;
        state.total_shares = state
            .total_shares
            .checked_sub(shares)
            .ok_or(VaultError::Overflow)?;

        emit!(WithdrawEvent {
            user: ctx.accounts.user.key(),
            shares_burned: shares,
            asset_amount,
            timestamp: Clock::get()?.unix_timestamp,
        });
        msg!("WithdrawEvent finished!");
        Ok(())
    }

    // ─────────────────────────────────────────
    // ADMIN: pause / unpause vault
    // ─────────────────────────────────────────
    pub fn set_pause(ctx: Context<AdminAction>, pause: bool) -> Result<()> {
        ctx.accounts.vault_state.paused = pause;
        Ok(())
    }
}

// ╭────────────────────────────────────────────
// │              ACCOUNT STATE                 │
// ╰────────────────────────────────────────────
#[account]
pub struct VaultState {
    /*  8 */ pub asset_mint: Pubkey,
    /* 40 */ pub share_mint: Pubkey,
    /* 72 */ pub vault_authority: Pubkey,
    /*104 */ pub vault_asset_account: Pubkey,
    /*136 */ pub admin: Pubkey,
    /*168 */ pub total_asset: u64,
    /*176 */ pub total_shares: u64,
    /*184 */ pub paused: bool,
    /*185 */ _padding: [u8; 7],
}
const _VAULT_STATE_SIZE: usize = 8 + 32 * 5 + 8 + 8 + 1 + 7; // = 185

// ╭────────────────────────────────────────────
// │                EVENTS                      │
// ╰────────────────────────────────────────────
#[event]
pub struct DepositEvent {
    pub user: Pubkey,
    pub asset_amount: u64,
    pub shares_minted: u64,
    pub timestamp: i64,
}

#[event]
pub struct WithdrawEvent {
    pub user: Pubkey,
    pub shares_burned: u64,
    pub asset_amount: u64,
    pub timestamp: i64,
}

// ╭────────────────────────────────────────────
// │                ERRORS                      │
// ╰────────────────────────────────────────────
#[error_code]
pub enum VaultError {
    #[msg("Amount must be greater than zero.")]
    InvalidAmount,
    #[msg("Shares value invalid.")]
    InvalidShares,
    #[msg("Vault is paused.")]
    VaultPaused,
    #[msg("Math overflow / underflow.")]
    Overflow,
    #[msg("Resulting amount is zero due to rounding.")]
    RoundingError,
}

// ╭────────────────────────────────────────────
// │              CONTEXTS                      │
// ╰────────────────────────────────────────────
#[derive(Accounts)]
#[instruction(vault_bump: u8, auth_bump: u8)]
pub struct InitializeVault<'info> {
    #[account(
        init,
        payer = admin,
        seeds = [b"vault_state", asset_mint.key().as_ref()],
        bump,
        space = _VAULT_STATE_SIZE
    )]
    pub vault_state: Account<'info, VaultState>,

    /// CHECK: PDA, only used as signer
    #[account(
        seeds = [b"vault_authority", vault_state.key().as_ref()],
        bump = auth_bump
    )]
    pub vault_authority: UncheckedAccount<'info>,

    #[account(
     init,
     payer = admin,                      // <── the fee payer
     seeds = [b"vault_asset", asset_mint.key().as_ref(), vault_state.key().as_ref()],
     bump,
    token::mint = asset_mint,
     token::authority = vault_authority
    )]
    pub vault_asset_account: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub admin: Signer<'info>,

    pub asset_mint: Box<Account<'info, Mint>>,
    #[account(
        init,
        payer = admin,
        mint::decimals = asset_mint.decimals,
        mint::authority = vault_authority,
        mint::freeze_authority = vault_authority
    )]
    pub share_mint: Box<Account<'info, Mint>>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub vault_state: Account<'info, VaultState>,

    #[account(mut)]
    pub vault_asset_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user_asset_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user_share_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub share_mint: Account<'info, Mint>,

    /// CHECK: signer PDA for CPIs
    #[account(
        seeds = [b"vault_authority", vault_state.key().as_ref()],
        bump
    )]
    pub vault_authority: UncheckedAccount<'info>,

    #[account(mut, signer)]
    pub user: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub vault_state: Account<'info, VaultState>,

    #[account(mut)]
    pub vault_asset_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user_asset_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user_share_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub share_mint: Account<'info, Mint>,

    /// CHECK: signer PDA for CPIs
    #[account(
        seeds = [b"vault_authority", vault_state.key().as_ref()],
        bump
    )]
    pub vault_authority: UncheckedAccount<'info>,

    #[account(mut, signer)]
    pub user: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct AdminAction<'info> {
    #[account(mut, has_one = admin)]
    pub vault_state: Account<'info, VaultState>,

    #[account()]
    pub admin: Signer<'info>,
}