#!/bin/bash

# default params
network=${network:-testnet}
account=${accountId:-flux-dev}
master=${master:-flux-dev}
initialBalance=${initialBalance:-5}

while [ $# -gt 0 ]; do

   if [[ $1 == *"--"* ]]; then
        param="${1/--/}"
        declare $param="$2"
        # echo $1 $2 // Optional to see the parameter:value result
   fi

  shift
done

NEAR_ENV=$network near delete $account $master
NEAR_ENV=$network near create-account $account --masterAccount $master --initialBalance $initialBalance
