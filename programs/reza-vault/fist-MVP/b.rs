use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer, MintTo, Burn};
use anchor_lang::solana_program::clock;

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

#[program]
pub mod asset_vault {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let vault_state = &mut ctx.accounts.vault_state;
        vault_state.authority = *ctx.accounts.authority.key;
        vault_state.asset_mint = *ctx.accounts.asset_mint.to_account_info().key;
        vault_state.share_mint = *ctx.accounts.share_mint.to_account_info().key;
        vault_state.vault_token_account = *ctx.accounts.vault_token_account.to_account_info().key;
        vault_state.total_assets = 0;
        vault_state.total_shares = 0;
        vault_state.is_paused = false;
        Ok(())
    }

    pub fn deposit_asset_a(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        require!(!ctx.accounts.vault_state.is_paused, VaultError::VaultIsPaused);
        require!(amount > 0, VaultError::ZeroDepositAmount);

        let vault_state = &mut ctx.accounts.vault_state;
        let total_assets = vault_state.total_assets;
        let total_shares = vault_state.total_shares;

        let shares_to_mint = if total_assets == 0 || total_shares == 0 {
            amount
        } else {
            (amount as u128)
                .checked_mul(total_shares as u128)
                .and_then(|res| res.checked_div(total_assets as u128))
                .and_then(|res| u64::try_from(res).ok())
                .ok_or(VaultError::CalculationError)?
        };

        require!(shares_to_mint > 0, VaultError::ZeroSharesMinted);

        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.user_token_account.to_account_info(),
                    to: ctx.accounts.vault_token_account.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            amount,
        )?;

        let seeds = &["share_mint".as_bytes(), &vault_state.key().to_bytes()[..32], &[ctx.bumps.share_mint]];
        let signer = &[&seeds[..]];

        token::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    mint: ctx.accounts.share_mint.to_account_info(),
                    to: ctx.accounts.user_share_account.to_account_info(),
                    authority: ctx.accounts.share_mint.to_account_info(),
                },
                signer
            ),
            shares_to_mint,
        )?;

        vault_state.total_assets = vault_state.total_assets.checked_add(amount).ok_or(VaultError::Overflow)?;
        vault_state.total_shares = vault_state.total_shares.checked_add(shares_to_mint).ok_or(VaultError::Overflow)?;

        emit!(DepositEvent {
            user: *ctx.accounts.user.key,
            asset_amount: amount,
            shares_minted: shares_to_mint,
            timestamp: clock::Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    pub fn withdraw_asset_a(ctx: Context<Withdraw>, shares: u64) -> Result<()> {
        require!(!ctx.accounts.vault_state.is_paused, VaultError::VaultIsPaused);
        require!(shares > 0, VaultError::ZeroWithdrawShares);

        let vault_state = &mut ctx.accounts.vault_state;
        let total_assets = vault_state.total_assets;
        let total_shares = vault_state.total_shares;

        let asset_amount_to_return = (shares as u128)
            .checked_mul(total_assets as u128)
            .and_then(|res| res.checked_div(total_shares as u128))
            .and_then(|res| u64::try_from(res).ok())
            .ok_or(VaultError::CalculationError)?;

        require!(asset_amount_to_return > 0, VaultError::ZeroAssetsReturned);

        token::burn(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Burn {
                    mint: ctx.accounts.share_mint.to_account_info(),
                    from: ctx.accounts.user_share_account.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            shares,
        )?;

        let seeds = &["vault_state".as_bytes(), &vault_state.key().to_bytes()[..32], &[ctx.bumps.vault_state]];
        let signer = &[&seeds[..]];

        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.vault_token_account.to_account_info(),
                    to: ctx.accounts.user_token_account.to_account_info(),
                    authority: ctx.accounts.vault_state.to_account_info(),
                },
                signer
            ),
            asset_amount_to_return,
        )?;

        vault_state.total_assets = vault_state.total_assets.checked_sub(asset_amount_to_return).ok_or(VaultError::Underflow)?;
        vault_state.total_shares = vault_state.total_shares.checked_sub(shares).ok_or(VaultError::Underflow)?;

        emit!(WithdrawEvent {
            user: *ctx.accounts.user.key,
            shares_burned: shares,
            asset_amount: asset_amount_to_return,
            timestamp: clock::Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    pub fn set_authority(ctx: Context<SetAuthority>, new_authority: Pubkey) -> Result<()> {
        ctx.accounts.vault_state.authority = new_authority;
        Ok(())
    }

    pub fn pause_vault(ctx: Context<ManageVault>) -> Result<()> {
        ctx.accounts.vault_state.is_paused = true;
        Ok(())
    }

    pub fn unpause_vault(ctx: Context<ManageVault>) -> Result<()> {
        ctx.accounts.vault_state.is_paused = false;
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        init,
        payer = authority,
        space = 8 + 32 + 32 + 32 + 32 + 8 + 8 + 1,
        seeds = ["vault_state".as_bytes(), authority.key().as_ref()],
        bump
    )]
    pub vault_state: Account<'info, VaultState>,
    pub asset_mint: Account<'info, Mint>,
    #[account(
        init,
        payer = authority,
        mint::decimals = asset_mint.decimals,
        mint::authority = share_mint,
        seeds = ["share_mint".as_bytes(), vault_state.key().as_ref()],
        bump
    )]
    pub share_mint: Account<'info, Mint>,
    #[account(
        init,
        payer = authority,
        token::mint = asset_mint,
        token::authority = vault_state,
        seeds = ["vault_token_account".as_bytes(), vault_state.key().as_ref()],
        bump
    )]
    pub vault_token_account: Account<'info, TokenAccount>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(
        mut,
        has_one = asset_mint,
        has_one = share_mint,
        has_one = vault_token_account
    )]
    pub vault_state: Account<'info, VaultState>,
    pub asset_mint: Account<'info, Mint>,
    #[account(mut, seeds = ["share_mint".as_bytes(), vault_state.key().as_ref()], bump)]
    pub share_mint: Account<'info, Mint>,
    #[account(
        mut,
        constraint = user_token_account.mint == *asset_mint.to_account_info().key
    )]
    pub user_token_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint = user_share_account.mint == *share_mint.to_account_info().key
    )]
    pub user_share_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub vault_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(
        mut,
        seeds = ["vault_state".as_bytes(), vault_state.authority.as_ref()],
        bump,
        has_one = authority,
        has_one = asset_mint,
        has_one = share_mint,
        has_one = vault_token_account,
    )]
    pub vault_state: Account<'info, VaultState>,
    /// CHECK: This is safe because it's a PDA derived from the vault state.
    #[account(seeds = ["vault_state".as_bytes(), vault_state.key().as_ref()], bump)]
    pub authority: UncheckedAccount<'info>,
    pub asset_mint: Account<'info, Mint>,
    #[account(mut, seeds = ["share_mint".as_bytes(), vault_state.key().as_ref()], bump)]
    pub share_mint: Account<'info, Mint>,
    #[account(
        mut,
        constraint = user_token_account.mint == *asset_mint.to_account_info().key
    )]
    pub user_token_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint = user_share_account.mint == *share_mint.to_account_info().key
    )]
    pub user_share_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub vault_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}


#[derive(Accounts)]
pub struct SetAuthority<'info> {
    #[account(
        mut,
        has_one = authority
    )]
    pub vault_state: Account<'info, VaultState>,
    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct ManageVault<'info> {
    #[account(
        mut,
        has_one = authority
    )]
    pub vault_state: Account<'info, VaultState>,
    pub authority: Signer<'info>,
}

#[account]
pub struct VaultState {
    pub authority: Pubkey,
    pub asset_mint: Pubkey,
    pub share_mint: Pubkey,
    pub vault_token_account: Pubkey,
    pub total_assets: u64,
    pub total_shares: u64,
    pub is_paused: bool,
}

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

#[error_code]
pub enum VaultError {
    #[msg("Vault is currently paused.")]
    VaultIsPaused,
    #[msg("Deposit amount must be greater than zero.")]
    ZeroDepositAmount,
    #[msg("Shares to mint must be greater than zero.")]
    ZeroSharesMinted,
    #[msg("Withdraw shares must be greater than zero.")]
    ZeroWithdrawShares,
    #[msg("Assets to return must be greater than zero.")]
    ZeroAssetsReturned,
    #[msg("Calculation error occurred.")]
    CalculationError,
    #[msg("Overflow occurred.")]
    Overflow,
    #[msg("Underflow occurred.")]
    Underflow,
}
