## Setup Bash variables

Change to your account (set up through [near-cli](https://docs.near.org/docs/tools/near-cli)):

```bash
ACCOUNT=account.testnet
ORACLE=oracle.account.testnet
REQUESTOR=requestor.account.testnet
```

## Deployment

The default parameters of `deploy_oracle.sh` are listed inside the script; change them with shell arguments, e.g. `--validityBond 1`. Testnet is used unless specified otherwise. Example deployment using the shell variables at the top:

```bash
bash deployment/deploy_oracle.sh --accountId $ORACLE --gov $ACCOUNT

bash deployment/deploy_requestor.sh --accountId $REQUESTOR
```

## Reset account

Example resetting (deleting then creating) the oracle account:

```bash
bash deployment/reset_account.sh --master $ACCOUNT --account $ORACLE
```
