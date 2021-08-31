#!/bin/bash

# default params
network=${network:-testnet}
accountId=${accountId:-oracle.account.testnet}
gov=${gov:-flux-dev}
finalArbitrator=${finalArbitrator:-flux-dev}
stakeToken=${stakeToken:-v2.wnear.flux-dev}
paymentToken=${paymentToken:-v2.wnear.flux-dev}
validityBond=${validityBond:-1000000000000000000000000}
maxOutcomes=${maxOutcomes:-8}
defaultChallengeWindowDuration=${defaultChallengeWindowDuration:-120000000000}
minInitialChallengeWindowDuration=${minInitialChallengeWindowDuration:-120000000000}
finalArbitratorInvokeAmount=${finalArbitratorInvokeAmount:-100000000000000000000000000}
# resolutionFeePercentage=${resolutionFeePercentage:-5000}
fluxMarketCap=${fluxMarketCap:-10000000000000}
totalValueStaked=${totalValueStaked:-1000}
resolutionFeePercentage=${totalValueStaked:-1000}

while [ $# -gt 0 ]; do

   if [[ $1 == *"--"* ]]; then
        param="${1/--/}"
        declare $param="$2"
        # echo $1 $2 // Optional to see the parameter:value result
   fi

  shift
done

NEAR_ENV=$network near deploy --accountId $accountId --wasmFile ./res/oracle.wasm --initFunction new --initArgs '{ "config": { "gov": "'$gov'", "final_arbitrator": "'$finalArbitrator'", "stake_token": "'$stakeToken'", "payment_token": "'$paymentToken'", "validity_bond": "'$validityBond'", "max_outcomes": '$maxOutcomes', "default_challenge_window_duration": "'$defaultChallengeWindowDuration'", "min_initial_challenge_window_duration": "'$minInitialChallengeWindowDuration'", "final_arbitrator_invoke_amount": "'$finalArbitratorInvokeAmount'", "resolution_fee_percentage": '$resolutionFeePercentage', "fee": {"flux_market_cap": "'$fluxMarketCap'", "total_value_staked":"'$totalValueStaked'", "resolution_fee_percentage": '$resolutionFeePercentage' } } }'
