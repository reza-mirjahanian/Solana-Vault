use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{burn, mint_to, transfer, Burn, Mint, MintTo, Token, TokenAccount, Transfer},
};

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

#[program]
pub mod asset_vault {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        vault_authority_bump: u8,
    ) -> Result<()> {
        ctx.accounts.state.admin = *ctx.accounts.admin.key;
        ctx.accounts.state.share_mint = ctx.accounts.share_mint.key();
        ctx.accounts.state.asset_token_mint = ctx.accounts.asset_token_mint.key();
        ctx.accounts.state.vault_token_account = ctx.accounts.vault_token_account.key();
        ctx.accounts.state.vault_authority_bump = vault_authority_bump;
        ctx.accounts.state.paused = false;
        Ok(())
    }

    pub fn deposit_asset_a(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        require!(!ctx.accounts.state.paused, VaultError::VaultPaused);
        require!(amount > 0, VaultError::InvalidAmount);

        let cpi_accounts = Transfer {
            from: ctx.accounts.user_asset_a_ata.to_account_info(),
            to: ctx.accounts.vault_token_account.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
        );
        transfer(cpi_ctx, amount)?;

        let total_supply = ctx.accounts.share_mint.supply;
        let vault_balance = ctx.accounts.vault_token_account.amount;
        let shares = if total_supply == 0 {
            amount
        } else {
            amount
                .checked_mul(total_supply)
                .unwrap()
                .checked_div(vault_balance - amount)
                .unwrap()
        };

        let seeds = &[
            b"vault_authority",
            &[ctx.accounts.state.vault_authority_bump],
        ];
        let signer = &[&seeds[..]];

        let cpi_accounts = MintTo {
            mint: ctx.accounts.share_mint.to_account_info(),
            to: ctx.accounts.user_share_ata.to_account_info(),
            authority: ctx.accounts.vault_authority.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer,
        );
        mint_to(cpi_ctx, shares)?;

        emit!(DepositEvent {
            user: *ctx.accounts.user.key,
            asset_amount: amount,
            shares_minted: shares,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    pub fn withdraw_asset_a(ctx: Context<Withdraw>, shares: u64) -> Result<()> {
        require!(!ctx.accounts.state.paused, VaultError::VaultPaused);
        require!(shares > 0, VaultError::InvalidAmount);
        require!(
            ctx.accounts.user_share_ata.amount >= shares,
            VaultError::InsufficientShares
        );

        let total_supply = ctx.accounts.share_mint.supply;
        let vault_balance = ctx.accounts.vault_token_account.amount;
        let amount = shares
            .checked_mul(vault_balance)
            .unwrap()
            .checked_div(total_supply)
            .unwrap();

        let seeds = &[
            b"vault_authority",
            &[ctx.accounts.state.vault_authority_bump],
        ];
        let signer = &[&seeds[..]];

        let cpi_accounts = Burn {
            mint: ctx.accounts.share_mint.to_account_info(),
            from: ctx.accounts.user_share_ata.to_account_info(),
            authority: ctx.accounts.vault_authority.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer,
        );
        burn(cpi_ctx, shares)?;

        let cpi_accounts = Transfer {
            from: ctx.accounts.vault_token_account.to_account_info(),
            to: ctx.accounts.user_asset_a_ata.to_account_info(),
            authority: ctx.accounts.vault_authority.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer,
        );
        transfer(cpi_ctx, amount)?;

        emit!(WithdrawEvent {
            user: *ctx.accounts.user.key,
            shares_burned: shares,
            asset_amount: amount,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    pub fn pause_vault(ctx: Context<AdminControl>) -> Result<()> {
        require!(
            ctx.accounts.admin.key() == &ctx.accounts.state.admin,
            VaultError::Unauthorized
        );
        ctx.accounts.state.paused = true;
        Ok(())
    }

    pub fn unpause_vault(ctx: Context<AdminControl>) -> Result<()> {
        require!(
            ctx.accounts.admin.key() == &ctx.accounts.state.admin,
            VaultError::Unauthorized
        );
        ctx.accounts.state.paused = false;
        Ok(())
    }

    pub fn set_admin(ctx: Context<AdminControl>, new_admin: Pubkey) -> Result<()> {
        require!(
            ctx.accounts.admin.key() == &ctx.accounts.state.admin,
            VaultError::Unauthorized
        );
        ctx.accounts.state.admin = new_admin;
        Ok(())
    }
}

#[account]
pub struct VaultState {
    admin: Pubkey,
    share_mint: Pubkey,
    asset_token_mint: Pubkey,
    vault_token_account: Pubkey,
    vault_authority_bump: u8,
    paused: bool,
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = admin, space = 8 + 32 * 4 + 1 + 1)]
    pub state: Account<'info, VaultState>,
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        init,
        payer = admin,
        mint::decimals = 0,
        mint::authority = vault_authority,
    )]
    pub share_mint: Account<'info, Mint>,
    pub asset_token_mint: Account<'info, Mint>,
    #[account(
        init,
        payer = admin,
        token::mint = asset_token_mint,
        token::authority = vault_authority,
    )]
    pub vault_token_account: Account<'info, TokenAccount>,
    #[account(seeds = [b"vault_authority"], bump)]
    pub vault_authority: AccountInfo<'info>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub state: Account<'info, VaultState>,
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut)]
    pub user_asset_a_ata: Account<'info, TokenAccount>,
    #[account(
        mut,
        address = state.vault_token_account
    )]
    pub vault_token_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        address = state.share_mint
    )]
    pub share_mint: Account<'info, Mint>,
    #[account(
        mut,
        associated_token::mint = share_mint,
        associated_token::authority = user
    )]
    pub user_share_ata: Account<'info, TokenAccount>,
    #[account(seeds = [b"vault_authority"], bump = state.vault_authority_bump)]
    pub vault_authority: AccountInfo<'info>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub state: Account<'info, VaultState>,
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut)]
    pub user_asset_a_ata: Account<'info, TokenAccount>,
    #[account(
        mut,
        address = state.vault_token_account
    )]
    pub vault_token_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        address = state.share_mint
    )]
    pub share_mint: Account<'info, Mint>,
    #[account(
        mut,
        associated_token::mint = share_mint,
        associated_token::authority = user
    )]
    pub user_share_ata: Account<'info, TokenAccount>,
    #[account(seeds = [b"vault_authority"], bump = state.vault_authority_bump)]
    pub vault_authority: AccountInfo<'info>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AdminControl<'info> {
    #[account(mut)]
    pub state: Account<'info, VaultState>,
    pub admin: Signer<'info>,
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
    #[msg("Insufficient shares")]
    InsufficientShares,
    #[msg("Unauthorized access")]
    Unauthorized,
}