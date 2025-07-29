use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer, MintTo, Burn};

declare_id!("11111111111111111111111111111112");

#[program]
pub mod asset_vault {
    use super::*;

    pub fn initialize_vault(
        ctx: Context<InitializeVault>,
        vault_bump: u8,
    ) -> Result<()> {
        let vault = &mut ctx.accounts.vault;
        vault.authority = ctx.accounts.authority.key();
        vault.asset_mint = ctx.accounts.asset_mint.key();
        vault.shares_mint = ctx.accounts.shares_mint.key();
        vault.vault_token_account = ctx.accounts.vault_token_account.key();
        vault.total_shares = 0;
        vault.total_assets = 0;
        vault.paused = false;
        vault.bump = vault_bump;
        Ok(())
    }

    pub fn deposit_asset_a(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        require!(!ctx.accounts.vault.paused, ErrorCode::VaultPaused);
        require!(amount > 0, ErrorCode::InvalidAmount);

        let vault = &mut ctx.accounts.vault;
        let shares_to_mint = if vault.total_shares == 0 {
            amount
        } else {
            amount
                .checked_mul(vault.total_shares)
                .unwrap()
                .checked_div(vault.total_assets)
                .unwrap()
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

        let seeds = &[
            b"vault",
            vault.asset_mint.as_ref(),
            &[vault.bump],
        ];
        let signer = &[&seeds[..]];

        token::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    mint: ctx.accounts.shares_mint.to_account_info(),
                    to: ctx.accounts.user_shares_account.to_account_info(),
                    authority: ctx.accounts.vault.to_account_info(),
                },
                signer,
            ),
            shares_to_mint,
        )?;

        vault.total_assets = vault.total_assets.checked_add(amount).unwrap();
        vault.total_shares = vault.total_shares.checked_add(shares_to_mint).unwrap();

        emit!(DepositEvent {
            user: ctx.accounts.user.key(),
            asset_amount: amount,
            shares_minted: shares_to_mint,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    pub fn withdraw_asset_a(ctx: Context<Withdraw>, shares: u64) -> Result<()> {
        require!(!ctx.accounts.vault.paused, ErrorCode::VaultPaused);
        require!(shares > 0, ErrorCode::InvalidAmount);

        let vault = &mut ctx.accounts.vault;
        require!(vault.total_shares > 0, ErrorCode::NoShares);

        let asset_amount = shares
            .checked_mul(vault.total_assets)
            .unwrap()
            .checked_div(vault.total_shares)
            .unwrap();

        token::burn(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Burn {
                    mint: ctx.accounts.shares_mint.to_account_info(),
                    from: ctx.accounts.user_shares_account.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            shares,
        )?;

        let seeds = &[
            b"vault",
            vault.asset_mint.as_ref(),
            &[vault.bump],
        ];
        let signer = &[&seeds[..]];

        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.vault_token_account.to_account_info(),
                    to: ctx.accounts.user_token_account.to_account_info(),
                    authority: ctx.accounts.vault.to_account_info(),
                },
                signer,
            ),
            asset_amount,
        )?;

        vault.total_assets = vault.total_assets.checked_sub(asset_amount).unwrap();
        vault.total_shares = vault.total_shares.checked_sub(shares).unwrap();

        emit!(WithdrawEvent {
            user: ctx.accounts.user.key(),
            shares_burned: shares,
            asset_amount,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    pub fn pause_vault(ctx: Context<AdminAction>) -> Result<()> {
        ctx.accounts.vault.paused = true;
        Ok(())
    }

    pub fn unpause_vault(ctx: Context<AdminAction>) -> Result<()> {
        ctx.accounts.vault.paused = false;
        Ok(())
    }

    pub fn set_admin(ctx: Context<AdminAction>, new_admin: Pubkey) -> Result<()> {
        ctx.accounts.vault.authority = new_admin;
        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(vault_bump: u8)]
pub struct InitializeVault<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + Vault::INIT_SPACE,
        seeds = [b"vault", asset_mint.key().as_ref()],
        bump = vault_bump
    )]
    pub vault: Account<'info, Vault>,
    
    #[account(mut)]
    pub authority: Signer<'info>,
    
    pub asset_mint: Account<'info, Mint>,
    
    #[account(
        init,
        payer = authority,
        mint::decimals = asset_mint.decimals,
        mint::authority = vault,
        seeds = [b"shares", asset_mint.key().as_ref()],
        bump
    )]
    pub shares_mint: Account<'info, Mint>,
    
    #[account(
        init,
        payer = authority,
        token::mint = asset_mint,
        token::authority = vault,
        seeds = [b"vault_tokens", asset_mint.key().as_ref()],
        bump
    )]
    pub vault_token_account: Account<'info, TokenAccount>,
    
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(
        mut,
        seeds = [b"vault", vault.asset_mint.as_ref()],
        bump = vault.bump
    )]
    pub vault: Account<'info, Vault>,
    
    #[account(mut)]
    pub user: Signer<'info>,
    
    #[account(
        mut,
        token::mint = vault.asset_mint,
        token::authority = user
    )]
    pub user_token_account: Account<'info, TokenAccount>,
    
    #[account(
        mut,
        token::mint = vault.shares_mint,
        token::authority = user
    )]
    pub user_shares_account: Account<'info, TokenAccount>,
    
    #[account(
        mut,
        address = vault.vault_token_account
    )]
    pub vault_token_account: Account<'info, TokenAccount>,
    
    #[account(
        mut,
        address = vault.shares_mint
    )]
    pub shares_mint: Account<'info, Mint>,
    
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(
        mut,
        seeds = [b"vault", vault.asset_mint.as_ref()],
        bump = vault.bump
    )]
    pub vault: Account<'info, Vault>,
    
    #[account(mut)]
    pub user: Signer<'info>,
    
    #[account(
        mut,
        token::mint = vault.asset_mint,
        token::authority = user
    )]
    pub user_token_account: Account<'info, TokenAccount>,
    
    #[account(
        mut,
        token::mint = vault.shares_mint,
        token::authority = user
    )]
    pub user_shares_account: Account<'info, TokenAccount>,
    
    #[account(
        mut,
        address = vault.vault_token_account
    )]
    pub vault_token_account: Account<'info, TokenAccount>,
    
    #[account(
        mut,
        address = vault.shares_mint
    )]
    pub shares_mint: Account<'info, Mint>,
    
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct AdminAction<'info> {
    #[account(
        mut,
        has_one = authority,
        seeds = [b"vault", vault.asset_mint.as_ref()],
        bump = vault.bump
    )]
    pub vault: Account<'info, Vault>,
    
    pub authority: Signer<'info>,
}

#[account]
#[derive(InitSpace)]
pub struct Vault {
    pub authority: Pubkey,
    pub asset_mint: Pubkey,
    pub shares_mint: Pubkey,
    pub vault_token_account: Pubkey,
    pub total_shares: u64,
    pub total_assets: u64,
    pub paused: bool,
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
    #[msg("Vault is paused")]
    VaultPaused,
    #[msg("Invalid amount")]
    InvalidAmount,
    #[msg("No shares exist")]
    NoShares,
}