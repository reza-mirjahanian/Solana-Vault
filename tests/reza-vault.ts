import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import {
    TOKEN_PROGRAM_ID,
    createMint,
    getOrCreateAssociatedTokenAccount,
    mintTo,
    getAccount,
} from "@solana/spl-token";
import { assert } from "chai";
import { RezaVault } from "../target/types/reza_vault";

const DECIMALS = 6;
const UNIT = 10 ** DECIMALS; // smallest unit for convenience


/**
 * Utility function to parse the program logs and extract event data
 * @param logs - Array of log messages from the transaction
 * @param eventName - The name of the event to extract (e.g., "DepositEvent")
 */
export const parseProgramLogs = (logs: string[], eventName: string) => {
    // Look for event log entries that contain the event name
    const eventLog = logs.find((log) => log.includes(eventName));
    console.log(eventLog)
    if (!eventLog) {
        throw new Error(`${eventName} not emitted`);
    }
    return eventLog;
};

describe("reza-vault", () => {
    // ─────────────────────────────────────────────
    //  Boiler-plate
    // ─────────────────────────────────────────────
    anchor.setProvider(anchor.AnchorProvider.env());
    const provider = anchor.getProvider<anchor.AnchorProvider>();
    const connection = provider.connection;
    const program = anchor.workspace.RezaVault as Program<RezaVault>;

    // Signers
    const admin = provider.wallet;               // the local test wallet
    const user = anchor.web3.Keypair.generate(); // random user

    // PDA + token addresses (filled in during setup)
    let assetMint: anchor.web3.PublicKey;
    let shareMintKP = anchor.web3.Keypair.generate();
    let shareMint: anchor.web3.PublicKey;

    let vaultState: anchor.web3.PublicKey;
    let vaultAuthority: anchor.web3.PublicKey;
    let vaultAssetAccount: anchor.web3.PublicKey;

    let userAssetAccount: anchor.web3.PublicKey;
    let userShareAccount: anchor.web3.PublicKey;

    // PDA bumps
    let stateBump: number;
    let authBump: number;
    let vaultAssetBump: number;

    // ─────────────────────────────────────────────
    //  One-time setup
    // ─────────────────────────────────────────────
    before(async () => {
        // Airdrop SOL so our user can pay fees
        await connection.requestAirdrop(
            user.publicKey,
            anchor.web3.LAMPORTS_PER_SOL * 2
        );

        // Create Asset-A mint (6 decimals, admin is mint authority)
        assetMint = await createMint(
            connection,
            (admin as any).payer,           // anchor's local wallet exposes .payer
            admin.publicKey,
            null,
            DECIMALS
        );

        // Derive PDAs exactly as the program does
        [vaultState, stateBump] =
            anchor.web3.PublicKey.findProgramAddressSync(
                [Buffer.from("vault_state"), assetMint.toBuffer()],
                program.programId
            );

        [vaultAuthority, authBump] =
            anchor.web3.PublicKey.findProgramAddressSync(
                [Buffer.from("vault_authority"), vaultState.toBuffer()],
                program.programId
            );

        [vaultAssetAccount, vaultAssetBump] =
            anchor.web3.PublicKey.findProgramAddressSync(
                [
                    Buffer.from("vault_asset"),
                    assetMint.toBuffer(),
                    vaultState.toBuffer(),
                ],
                program.programId
            );

        // Create an ATA for our user’s Asset-A tokens
        userAssetAccount = (
            await getOrCreateAssociatedTokenAccount(
                connection,
                (admin as any).payer,
                assetMint,
                user.publicKey
            )
        ).address;

        // Mint some Asset-A to the user (𝟏 000 tokens)
        await mintTo(
            connection,
            (admin as any).payer,
            assetMint,
            userAssetAccount,
            admin.publicKey,
            1_000 * UNIT
        );

        // ~~~~~~~~~ Initialise the vault ~~~~~~~~~
        await program.methods
            .initializeVault(stateBump, authBump)
            .accounts({
                vaultState,
                vaultAuthority,
                vaultAssetAccount,
                admin: admin.publicKey,
                assetMint,
                shareMint: shareMintKP.publicKey,
                systemProgram: anchor.web3.SystemProgram.programId,
                tokenProgram: TOKEN_PROGRAM_ID,
                rent: anchor.web3.SYSVAR_RENT_PUBKEY,
            })
            .signers([shareMintKP])
            .rpc();

        shareMint = shareMintKP.publicKey;

        // Create user’s Share-token ATA
        userShareAccount = (
            await getOrCreateAssociatedTokenAccount(
                connection,
                (admin as any).payer,
                shareMint,
                user.publicKey
            )
        ).address;
    });

    // ─────────────────────────────────────────────
    //  Helper utilities for assertions
    // ─────────────────────────────────────────────
    const fetchVaultState = async () =>
        (await program.account.vaultState.fetch(vaultState)) as any;

    const getTokenBalance = async (acc: anchor.web3.PublicKey) =>
        Number((await getAccount(connection, acc)).amount);

    // ─────────────────────────────────────────────
    //  Tests
    // ─────────────────────────────────────────────

    it("initialises with zero totals & correct config", async () => {
        const state = await fetchVaultState();
        assert.ok(state.totalAsset.toNumber() === 0);
        assert.ok(state.totalShares.toNumber() === 0);
        assert.ok(state.assetMint.equals(assetMint));
        assert.ok(state.shareMint.equals(shareMint));
        assert.ok(state.vaultAssetAccount.equals(vaultAssetAccount));
        assert.ok(state.vaultAuthority.equals(vaultAuthority));
    });

    it("deposits 100 tokens and mints 100 shares", async () => {
        const amount = new anchor.BN(100 * UNIT);

          const confirmedConn = new anchor.web3.Connection(
                connection.rpcEndpoint,
                "confirmed",
              );


        // Send tx
        const txSig = await program.methods
            .depositAssetA(amount)
            .accounts({
                vaultState,
                vaultAssetAccount,
                userAssetAccount,
                userShareAccount,
                shareMint,
                vaultAuthority,
                user: user.publicKey,
                tokenProgram: TOKEN_PROGRAM_ID,
            })
            .signers([user])
            .rpc();

        // Fetch the transaction's logs
        await confirmedConn.confirmTransaction(txSig, "confirmed"); // optional but neat
        const tx = await confirmedConn.getTransaction(txSig, {
            commitment: "confirmed",
            maxSupportedTransactionVersion: 0,
        });


        if (!tx || !tx.meta) {
            throw new Error("Transaction not found or failed");
        }

        const logs = tx.meta.logMessages || [];

        // Parse DepositEvent from logs
        let eventData;
        try {
            eventData = parseProgramLogs(logs, "DepositEvent");
        } catch (err) {
            assert.fail("DepositEvent not emitted: " + err.message);
        }

        // ───────────────  state assertions  ───────────────
        const state = await fetchVaultState();
        assert.strictEqual(state.totalAsset.toNumber(), 100 * UNIT);
        assert.strictEqual(state.totalShares.toNumber(), 100 * UNIT);

        const userShares  = await getTokenBalance(userShareAccount);
        const vaultAssets = await getTokenBalance(vaultAssetAccount);
        assert.strictEqual(userShares, 100 * UNIT);
        assert.strictEqual(vaultAssets, 100 * UNIT);

        // ───────────────  event assertions  ───────────────
        assert.ok(eventData, "Program log: DepositEvent finished!");
        //todo decode eventData

        // assert.strictEqual(new anchor.BN(eventData.assetAmount).toNumber(), 100 * UNIT);
        // assert.strictEqual(new anchor.BN(eventData.sharesMinted).toNumber(), 100 * UNIT);
    });


    it("second deposit (50 tokens) keeps share ratio intact", async () => {
        const amount = new anchor.BN(50 * UNIT);

        await program.methods
            .depositAssetA(amount)
            .accounts({
                vaultState,
                vaultAssetAccount,
                userAssetAccount,
                userShareAccount,
                shareMint,
                vaultAuthority,
                user: user.publicKey,
                tokenProgram: TOKEN_PROGRAM_ID,
            })
            .signers([user])
            .rpc();

        const state = await fetchVaultState();
        // After 100 + 50 deposit the vault should hold 150 units & 150 shares
        assert.strictEqual(state.totalAsset.toNumber(), 150 * UNIT);
        assert.strictEqual(state.totalShares.toNumber(), 150 * UNIT);

        const userShares = await getTokenBalance(userShareAccount);
        assert.strictEqual(userShares, 150 * UNIT);
    });

    it("withdraws 60 shares, receives 60 tokens", async () => {
        const shares = new anchor.BN(60 * UNIT);

        // Listen for WithdrawEvent
        const confirmedConn = new anchor.web3.Connection(
            connection.rpcEndpoint,
            "confirmed",
        );

        // Send tx
        const txSig = await program.methods
            .withdrawAssetA(shares)
            .accounts({
                vaultState,
                vaultAssetAccount,
                userAssetAccount,
                userShareAccount,
                shareMint,
                vaultAuthority,
                user: user.publicKey,
                tokenProgram: TOKEN_PROGRAM_ID,
            })
            .signers([user])
            .rpc();

        // Fetch the transaction's logs
        await confirmedConn.confirmTransaction(txSig, "confirmed"); // optional but neat
        const tx = await confirmedConn.getTransaction(txSig, {
            commitment: "confirmed",
            maxSupportedTransactionVersion: 0,
        });


        if (!tx || !tx.meta) {
            throw new Error("Transaction not found or failed");
        }

        const logs = tx.meta.logMessages || [];

        // Parse WithdrawEvent from logs
        let eventData;
        try {
            eventData = parseProgramLogs(logs, "WithdrawEvent");
        } catch (err) {
            assert.fail("WithdrawEvent not emitted: " + err.message);
        }

        const state = await fetchVaultState();
        // Totals should now be 90 tokens / 90 shares
        assert.strictEqual(state.totalAsset.toNumber(), 90 * UNIT);
        assert.strictEqual(state.totalShares.toNumber(), 90 * UNIT);

        const userShares = await getTokenBalance(userShareAccount);
        const userAssets = await getTokenBalance(userAssetAccount);
        const vaultAssets = await getTokenBalance(vaultAssetAccount);

        assert.strictEqual(userShares, 90 * UNIT);
        assert.strictEqual(vaultAssets, 90 * UNIT);
        // User originally had 1 000 tokens, deposited 150, withdrew 60 ⇒ balance 910
        assert.strictEqual(userAssets, 910 * UNIT);

        // Event check
        assert.ok(eventData !== null, "WithdrawEvent not emitted");
        //todo decode eventData

        // assert.strictEqual(eventData.sharesBurned.toNumber(), 60 * UNIT);
        // assert.strictEqual(eventData.assetAmount.toNumber(), 60 * UNIT);
    });

    it("rejects zero-amount deposit", async () => {
        const zero = new anchor.BN(0);
        try {
            await program.methods
                .depositAssetA(zero)
                .accounts({
                    vaultState,
                    vaultAssetAccount,
                    userAssetAccount,
                    userShareAccount,
                    shareMint,
                    vaultAuthority,
                    user: user.publicKey,
                    tokenProgram: TOKEN_PROGRAM_ID,
                })
                .signers([user])
                .rpc();
            assert.fail("should have thrown");
        } catch (err: any) {
            assert.include(err.toString(), "InvalidAmount");
        }
    });

    it("admin can pause and unpause; operations fail while paused", async () => {
        // pause true
        await program.methods
            .setPause(true)
            .accounts({ vaultState, admin: admin.publicKey })
            .rpc();

        // any deposit should now fail
        try {
            await program.methods
                .depositAssetA(new anchor.BN(10 * UNIT))
                .accounts({
                    vaultState,
                    vaultAssetAccount,
                    userAssetAccount,
                    userShareAccount,
                    shareMint,
                    vaultAuthority,
                    user: user.publicKey,
                    tokenProgram: TOKEN_PROGRAM_ID,
                })
                .signers([user])
                .rpc();
            assert.fail("deposit should be blocked when paused");
        } catch (err: any) {
            assert.include(err.toString(), "VaultPaused");
        }

        // un-pause
        await program.methods
            .setPause(false)
            .accounts({ vaultState, admin: admin.publicKey })
            .rpc();

        // deposit succeeds again (sanity)
        await program.methods
            .depositAssetA(new anchor.BN(10 * UNIT))
            .accounts({
                vaultState,
                vaultAssetAccount,
                userAssetAccount,
                userShareAccount,
                shareMint,
                vaultAuthority,
                user: user.publicKey,
                tokenProgram: TOKEN_PROGRAM_ID,
            })
            .signers([user])
            .rpc();
    });

    it("non-admin cannot pause the vault", async () => {
        try {
            await program.methods
                .setPause(true)
                .accounts({ vaultState, admin: user.publicKey })
                .signers([user])
                .rpc();
            assert.fail("non-admin managed to pause");
        } catch (err: any) {
            assert.include(err.toString(), "ConstraintHasOne");
        }
    });
});
