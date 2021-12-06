#!/bin/bash

# default params
network=${network:-mainnet}
accountId=${accountId:-v1.fluxoracle.near}
gov=${gov:-flux.sputnik-dao.near}
finalArbitrator=${finalArbitrator:-flux.sputnik-dao.near}
stakeToken=${stakeToken:-0x3Ea8ea4237344C9931214796d9417Af1A1180770.factory.bridge.near} # usdsc
paymentToken=${paymentToken:-a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.factory.bridge.near} # usdsc
validityBond=${validityBond:-1} # 1-e12 USDC
maxOutcomes=${maxOutcomes:-8} 
defaultChallengeWindowDuration=${defaultChallengeWindowDuration:-43200000000000} # 12hr
minInitialChallengeWindowDuration=${minInitialChallengeWindowDuration:-180000000000} # 12hr
finalArbitratorInvokeAmount=${finalArbitratorInvokeAmount:-100000000000000000000000} # 100K FLX
fluxMarketCap=${fluxMarketCap:-10000000000000}
totalValueStaked=${totalValueStaked:-0}
resolutionFeePercentage=${resolutionFeePercentage:-100} # 0.1%
minResolutionBond=${min_resolution_bond:-100000000000000000000} # 100 FLX

while [ $# -gt 0 ]; do

   if [[ $1 == *"--"* ]]; then
        param="${1/--/}"
        declare $param="$2"
        # echo $1 $2 // Optional to see the parameter:value result
   fi

  shift
done

NEAR_ENV=$network near deploy --accountId $accountId --wasmFile ./res/oracle.wasm --initFunction new --initArgs '{"initial_whitelist": [], "config": { "gov": "'$gov'", "final_arbitrator": "'$finalArbitrator'", "stake_token": "'$stakeToken'", "payment_token": "'$paymentToken'", "validity_bond": "'$validityBond'", "max_outcomes": '$maxOutcomes', "default_challenge_window_duration": "'$defaultChallengeWindowDuration'", "min_initial_challenge_window_duration": "'$minInitialChallengeWindowDuration'", "final_arbitrator_invoke_amount": "'$finalArbitratorInvokeAmount'", "resolution_fee_percentage": '$resolutionFeePercentage', "min_resolution_bond": "'$minResolutionBond'". "fee": {"flux_market_cap": "'$fluxMarketCap'", "total_value_staked":"'$totalValueStaked'", "resolution_fee_percentage": '$resolutionFeePercentage' } } }'
