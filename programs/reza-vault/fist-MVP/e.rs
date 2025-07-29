use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Mint, Transfer, Burn, MintTo};

declare_id!("11111111111111111111111111111111");

#[program]
pub mod asset_vault {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let vault = &mut ctx.accounts.vault;
        vault.admin = ctx.accounts.admin.key();
        vault.asset_mint = ctx.accounts.asset_mint.key();
        vault.share_mint = ctx.accounts.share_mint.key();
        vault.vault_authority = ctx.accounts.vault_authority.key();
        vault.vault_token_account = ctx.accounts.vault_token_account.key();
        vault.total_shares = 0;
        vault.is_paused = false;
        vault.bump = ctx.bumps.vault_authority;
        Ok(())
    }

    pub fn deposit_asset_a(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        require!(!ctx.accounts.vault.is_paused, ErrorCode::VaultPaused);
        require!(amount > 0, ErrorCode::InvalidAmount);

        let vault = &mut ctx.accounts.vault;
        let vault_balance = ctx.accounts.vault_token_account.amount;
        
        let shares_to_mint = if vault.total_shares == 0 {
            amount
        } else {
            amount.checked_mul(vault.total_shares)
                .ok_or(ErrorCode::MathOverflow)?
                .checked_div(vault_balance)
                .ok_or(ErrorCode::MathOverflow)?
        };

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

        token::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    mint: ctx.accounts.share_mint.to_account_info(),
                    to: ctx.accounts.user_share_account.to_account_info(),
                    authority: ctx.accounts.vault_authority.to_account_info(),
                },
                &[&[b"vault_authority", &[vault.bump]]],
            ),
            shares_to_mint,
        )?;

        vault.total_shares = vault.total_shares
            .checked_add(shares_to_mint)
            .ok_or(ErrorCode::MathOverflow)?;

        emit!(DepositEvent {
            user: ctx.accounts.user.key(),
            asset_amount: amount,
            shares_minted: shares_to_mint,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    pub fn withdraw_asset_a(ctx: Context<Withdraw>, shares: u64) -> Result<()> {
        require!(!ctx.accounts.vault.is_paused, ErrorCode::VaultPaused);
        require!(shares > 0, ErrorCode::InvalidAmount);
        require!(ctx.accounts.user_share_account.amount >= shares, ErrorCode::InsufficientShares);

        let vault = &mut ctx.accounts.vault;
        let vault_balance = ctx.accounts.vault_token_account.amount;
        
        let asset_amount = shares
            .checked_mul(vault_balance)
            .ok_or(ErrorCode::MathOverflow)?
            .checked_div(vault.total_shares)
            .ok_or(ErrorCode::MathOverflow)?;

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

        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.vault_token_account.to_account_info(),
                    to: ctx.accounts.user_token_account.to_account_info(),
                    authority: ctx.accounts.vault_authority.to_account_info(),
                },
                &[&[b"vault_authority", &[vault.bump]]],
            ),
            asset_amount,
        )?;

        vault.total_shares = vault.total_shares
            .checked_sub(shares)
            .ok_or(ErrorCode::MathOverflow)?;

        emit!(WithdrawEvent {
            user: ctx.accounts.user.key(),
            shares_burned: shares,
            asset_amount,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    pub fn pause_vault(ctx: Context<AdminAction>) -> Result<()> {
        ctx.accounts.vault.is_paused = true;
        Ok(())
    }

    pub fn unpause_vault(ctx: Context<AdminAction>) -> Result<()> {
        ctx.accounts.vault.is_paused = false;
        Ok(())
    }

    pub fn set_admin(ctx: Context<AdminAction>, new_admin: Pubkey) -> Result<()> {
        ctx.accounts.vault.admin = new_admin;
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = admin,
        space = 8 + 32 + 32 + 32 + 32 + 32 + 8 + 1 + 1,
        seeds = [b"vault"],
        bump
    )]
    pub vault: Account<'info, Vault>,
    #[account(mut)]
    pub admin: Signer<'info>,
    pub asset_mint: Account<'info, Mint>,
    #[account(
        init,
        payer = admin,
        mint::decimals = 6,
        mint::authority = vault_authority,
        mint::freeze_authority = vault_authority,
    )]
    pub share_mint: Account<'info, Mint>,
    #[account(
        seeds = [b"vault_authority"],
        bump,
    )]
    pub vault_authority: SystemAccount<'info>,
    #[account(
        init,
        payer = admin,
        token::mint = asset_mint,
        token::authority = vault_authority,
    )]
    pub vault_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(
        mut,
        seeds = [b"vault"],
        bump,
    )]
    pub vault: Account<'info, Vault>,
    #[account(
        mut,
        constraint = user_token_account.owner == user.key(),
        constraint = user_token_account.mint == vault.asset_mint,
    )]
    pub user_token_account: Account<'info, TokenAccount>,
    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = share_mint,
        associated_token::authority = user,
    )]
    pub user_share_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint = vault_token_account.key() == vault.vault_token_account,
    )]
    pub vault_token_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint = share_mint.key() == vault.share_mint,
    )]
    pub share_mint: Account<'info, Mint>,
    #[account(
        seeds = [b"vault_authority"],
        bump = vault.bump,
    )]
    pub vault_authority: SystemAccount<'info>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, anchor_spl::associated_token::AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(
        mut,
        seeds = [b"vault"],
        bump,
    )]
    pub vault: Account<'info, Vault>,
    #[account(
        mut,
        constraint = user_token_account.owner == user.key(),
        constraint = user_token_account.mint == vault.asset_mint,
    )]
    pub user_token_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint = user_share_account.owner == user.key(),
        constraint = user_share_account.mint == vault.share_mint,
    )]
    pub user_share_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint = vault_token_account.key() == vault.vault_token_account,
    )]
    pub vault_token_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint = share_mint.key() == vault.share_mint,
    )]
    pub share_mint: Account<'info, Mint>,
    #[account(
        seeds = [b"vault_authority"],
        bump = vault.bump,
    )]
    pub vault_authority: SystemAccount<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct AdminAction<'info> {
    #[account(
        constraint = admin.key() == vault.admin @ ErrorCode::Unauthorized
    )]
    pub admin: Signer<'info>,
    #[account(
        mut,
        seeds = [b"vault"],
        bump,
    )]
    pub vault: Account<'info, Vault>,
}

#[account]
pub struct Vault {
    pub admin: Pubkey,
    pub asset_mint: Pubkey,
    pub share_mint: Pubkey,
    pub vault_authority: Pubkey,
    pub vault_token_account: Pubkey,
    pub total_shares: u64,
    pub is_paused: bool,
    pub bump: u8,
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
pub enum ErrorCode {
    #[msg("Invalid amount")]
    InvalidAmount,
    #[msg("Math overflow")]
    MathOverflow,
    #[msg("Insufficient shares")]
    InsufficientShares,
    #[msg("Vault is paused")]
    VaultPaused,
    #[msg("Unauthorized")]
    Unauthorized,
}
