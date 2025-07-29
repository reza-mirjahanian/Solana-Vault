
! Read "Doc.md" file too :)



### Quick Installation:
```shell
curl --proto '=https' --tlsv1.2 -sSfL https://solana-install.solana.workers.dev | bash
```

```shell
Installed Versions:
Rust: rustc 1.85.0 (4d91de4e4 2025-02-17)
Solana CLI: solana-cli 2.1.15 (src:53545685; feat:3271415109, client:Agave)
Anchor CLI: anchor-cli 0.31.1
Node.js: v23.9.0
Yarn: 1.22.1

Installation complete. Please restart your terminal to apply all changes.
```
```shell
 npm install --legacy-peer-deps
```
-------

### Solana CLI

#### Solana Config:
```shell
solana config get
```


#### Create Wallet:
```shell
solana-keygen new
```

```shell
solana address
```


#### Airdrop of devnet SOL:

```shell
solana airdrop 2
```

```shell
solana balance
```
#### Run Local Validator:
The Solana CLI comes with the test validator built-in. Running a local validator will allow you to deploy and test your programs locally.
In a separate terminal, run the following command to start a local validator:
```shell
solana-test-validator
```


#### Update the Solana CLI cluster:
```shell
solana config set --url mainnet-beta
solana config set --url devnet
solana config set --url localhost
solana config set --url testnet
```

----------------
https://www.anchor-lang.com/docs

#### Initialize Project:
```shell
anchor init <project-name>
```

#### Build Program:
```shell
anchor build
```

#### Test Program:
```shell
anchor test
```

#### Deploy Program:
```shell
anchor deploy
```

```shell
anchor clean
```

