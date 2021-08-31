#!/bin/bash

# default params
network=${network:-testnet}
accountId=${accountId:-oracle.account.testnet}
oracle=${oracle:-flux-dev}
stakeToken=${stakeToken:-v2.wnear.flux-dev}

while [ $# -gt 0 ]; do

   if [[ $1 == *"--"* ]]; then
        param="${1/--/}"
        declare $param="$2"
        # echo $1 $2 // Optional to see the parameter:value result
   fi

  shift
done

NEAR_ENV=$network near deploy --accountId $accountId --wasmFile ./res/requestor_contracts.wasm --initFunction new --initArgs '{"oracle": "'$oracle'", "stake_token": "'$stakeToken'"}'