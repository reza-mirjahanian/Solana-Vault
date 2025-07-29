use anchor_lang::prelude::*;
use anchor_spl::{
    token::{self, Mint, Token, TokenAccount, Transfer, MintTo, Burn},
};

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

#[program]
pub mod asset_vault {
    use super::*;

    pub fn initialize_vault(
        ctx: Context<InitializeVault>,
        admin: Pubkey,
    ) -> Result<()> {
        let vault = &mut ctx.accounts.vault;
        vault.admin = admin;
        vault.paused = false;
        vault.total_assets = 0;
        vault.total_shares = 0;
        vault.asset_mint = ctx.accounts.asset_mint.key();
        vault.share_mint = ctx.accounts.share_mint.key();
        Ok(())
    }

    pub fn deposit_asset_a(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        require!(!ctx.accounts.vault.paused, VaultError::VaultPaused);
        require!(amount > 0, VaultError::InvalidAmount);

        let vault = &mut ctx.accounts.vault;
        let shares_to_mint = if vault.total_shares == 0 {
            amount
        } else {
            (amount as u128)
                .checked_mul(vault.total_shares as u128)
                .ok_or(VaultError::Overflow)?
                .checked_div(vault.total_assets as u128)
                .ok_or(VaultError::DivisionByZero)? as u64
        };

        let cpi_accounts = Transfer {
            from: ctx.accounts.user_asset_account.to_account_info(),
            to: ctx.accounts.vault_asset_account.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, amount)?;

        let cpi_accounts = MintTo {
            mint: ctx.accounts.share_mint.to_account_info(),
            to: ctx.accounts.user_share_account.to_account_info(),
            authority: ctx.accounts.vault_signer.to_account_info(),
        };
        let seeds = &[vault.to_account_info().key.as_ref(), &[vault.bump]];
        let signer = &[&seeds[..]];
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer,
        );
        token::mint_to(cpi_ctx, shares_to_mint)?;

        vault.total_assets = vault.total_assets.checked_add(amount).ok_or(VaultError::Overflow)?;
        vault.total_shares = vault.total_shares.checked_add(shares_to_mint).ok_or(VaultError::Overflow)?;

        emit!(DepositEvent {
            user: ctx.accounts.user.key(),
            asset_amount: amount,
            shares_minted: shares_to_mint,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    pub fn withdraw_asset_a(ctx: Context<Withdraw>, shares: u64) -> Result<()> {
        require!(!ctx.accounts.vault.paused, VaultError::VaultPaused);
        require!(shares > 0, VaultError::InvalidAmount);

        let vault = &mut ctx.accounts.vault;
        let amount = (shares as u128)
            .checked_mul(vault.total_assets as u128)
            .ok_or(VaultError::Overflow)?
            .checked_div(vault.total_shares as u128)
            .ok_or(VaultError::DivisionByZero)? as u64;

        let cpi_accounts = Burn {
            mint: ctx.accounts.share_mint.to_account_info(),
            from: ctx.accounts.user_share_account.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::burn(cpi_ctx, shares)?;

        let cpi_accounts = Transfer {
            from: ctx.accounts.vault_asset_account.to_account_info(),
            to: ctx.accounts.user_asset_account.to_account_info(),
            authority: ctx.accounts.vault_signer.to_account_info(),
        };
        let seeds = &[vault.to_account_info().key.as_ref(), &[vault.bump]];
        let signer = &[&seeds[..]];
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer,
        );
        token::transfer(cpi_ctx, amount)?;

        vault.total_assets = vault.total_assets.checked_sub(amount).ok_or(VaultError::Underflow)?;
        vault.total_shares = vault.total_shares.checked_sub(shares).ok_or(VaultError::Underflow)?;

        emit!(WithdrawEvent {
            user: ctx.accounts.user.key(),
            shares_burned: shares,
            asset_amount: amount,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    pub fn pause_vault(ctx: Context<AdminAction>) -> Result<()> {
        let vault = &mut ctx.accounts.vault;
        require!(ctx.accounts.admin.key() == vault.admin, VaultError::Unauthorized);
        vault.paused = true;
        Ok(())
    }

    pub fn unpause_vault(ctx: Context<AdminAction>) -> Result<()> {
        let vault = &mut ctx.accounts.vault;
        require!(ctx.accounts.admin.key() == vault.admin, VaultError::Unauthorized);
        vault.paused = false;
        Ok(())
    }

    pub fn set_admin(ctx: Context<SetAdmin>, new_admin: Pubkey) -> Result<()> {
        let vault = &mut ctx.accounts.vault;
        require!(ctx.accounts.admin.key() == vault.admin, VaultError::Unauthorized);
        vault.admin = new_admin;
        Ok(())
    }
}

#[derive(Accounts)]
pub struct InitializeVault<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + Vault::INIT_SPACE,
        seeds = [b"vault"],
        bump
    )]
    pub vault: Account<'info, Vault>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub asset_mint: Account<'info, Mint>,
    #[account(
        init,
        payer = payer,
        mint::decimals = 6,
        mint::authority = vault_signer,
    )]
    pub share_mint: Account<'info, Mint>,
    /// CHECK: This is safe because we're using it as a PDA signer
    #[account(
        seeds = [vault.key().as_ref()],
        bump,
    )]
    pub vault_signer: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(
        mut,
        seeds = [b"vault"],
        bump = vault.bump,
    )]
    pub vault: Account<'info, Vault>,
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(
        mut,
        constraint = asset_mint.key() == vault.asset_mint
    )]
    pub asset_mint: Account<'info, Mint>,
    #[account(
        mut,
        constraint = user_asset_account.owner == user.key(),
        constraint = user_asset_account.mint == asset_mint.key()
    )]
    pub user_asset_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint = vault_asset_account.owner == vault_signer.key(),
        constraint = vault_asset_account.mint == asset_mint.key()
    )]
    pub vault_asset_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint = share_mint.key() == vault.share_mint
    )]
    pub share_mint: Account<'info, Mint>,
    #[account(
        mut,
        constraint = user_share_account.owner == user.key(),
        constraint = user_share_account.mint == share_mint.key()
    )]
    pub user_share_account: Account<'info, TokenAccount>,
    /// CHECK: This is safe because we're using it as a PDA signer
    #[account(
        seeds = [vault.key().as_ref()],
        bump = vault.bump,
    )]
    pub vault_signer: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(
        mut,
        seeds = [b"vault"],
        bump = vault.bump,
    )]
    pub vault: Account<'info, Vault>,
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(
        mut,
        constraint = asset_mint.key() == vault.asset_mint
    )]
    pub asset_mint: Account<'info, Mint>,
    #[account(
        mut,
        constraint = user_asset_account.owner == user.key(),
        constraint = user_asset_account.mint == asset_mint.key()
    )]
    pub user_asset_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint = vault_asset_account.owner == vault_signer.key(),
        constraint = vault_asset_account.mint == asset_mint.key()
    )]
    pub vault_asset_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint = share_mint.key() == vault.share_mint
    )]
    pub share_mint: Account<'info, Mint>,
    #[account(
        mut,
        constraint = user_share_account.owner == user.key(),
        constraint = user_share_account.mint == share_mint.key()
    )]
    pub user_share_account: Account<'info, TokenAccount>,
    /// CHECK: This is safe because we're using it as a PDA signer
    #[account(
        seeds = [vault.key().as_ref()],
        bump = vault.bump,
    )]
    pub vault_signer: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct AdminAction<'info> {
    #[account(
        mut,
        seeds = [b"vault"],
        bump = vault.bump,
    )]
    pub vault: Account<'info, Vault>,
    pub admin: Signer<'info>,
}

#[derive(Accounts)]
pub struct SetAdmin<'info> {
    #[account(
        mut,
        seeds = [b"vault"],
        bump = vault.bump,
    )]
    pub vault: Account<'info, Vault>,
    pub admin: Signer<'info>,
}

#[account]
#[derive(InitSpace)]
pub struct Vault {
    pub admin: Pubkey,
    pub paused: bool,
    pub total_assets: u64,
    pub total_shares: u64,
    pub asset_mint: Pubkey,
    pub share_mint: Pubkey,
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
pub enum VaultError {
    #[msg("Vault is paused")]
    VaultPaused,
    #[msg("Invalid amount")]
    InvalidAmount,
    #[msg("Overflow")]
    Overflow,
    #[msg("Underflow")]
    Underflow,
    #[msg("Division by zero")]
    DivisionByZero,
    #[msg("Unauthorized")]
    Unauthorized,
}