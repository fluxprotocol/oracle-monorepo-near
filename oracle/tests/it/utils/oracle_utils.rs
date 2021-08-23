use crate::utils::*;
use oracle::oracle_config::OracleConfig;
use oracle::whitelist::{RequestorConfig};
use oracle::fee_config::FeeConfig;

pub struct OracleUtils {
    pub contract: ContractAccount<OracleContract>
}

fn new_registry_entry(contract_id: String, stake_multiplier: Option<u16>) -> RequestorConfig {
    RequestorConfig {
        code_base_url: None,
        contract_entry: contract_id,
        interface_name: "test".to_string(),
        stake_multiplier
    }
}

impl OracleUtils {
    pub fn new(
        master_account: &TestAccount,
        validity_bond: u128,
        final_arbitrator_invoke_amount: u128,
        stake_multiplier: Option<u16>
    ) -> Self {        
        let config = OracleConfig {
            gov: "alice".to_string(),
            final_arbitrator: "alice".to_string(),
            payment_token: TOKEN_CONTRACT_ID.to_string(),
            stake_token: TOKEN_CONTRACT_ID.to_string(),
            validity_bond: U128(validity_bond),
            max_outcomes: 8,
            default_challenge_window_duration: U64(1000),
            min_initial_challenge_window_duration: U64(1000),
            final_arbitrator_invoke_amount: U128(final_arbitrator_invoke_amount),
            fee: FeeConfig {
                flux_market_cap: U128(50000),
                total_value_staked: U128(10000),
                resolution_fee_percentage: 5000, // 5%
            }
        };
        
        // deploy token
        let contract = deploy!(
            // Contract Proxy
            contract: OracleContract,
            // Contract account id
            contract_id: ORACLE_CONTRACT_ID,
            // Bytes of contract
            bytes: &ORACLE_WASM_BYTES,
            // User deploying the contract,
            signer_account: master_account.account,
            deposit: to_yocto("1000"),
            // init method
            init_method: new(
                Some(vec![
                        new_registry_entry(REQUEST_INTERFACE_CONTRACT_ID.to_string(), stake_multiplier), 
                        new_registry_entry(TARGET_CONTRACT_ID.to_string(), stake_multiplier)
                    ]
                ), 
                config
            )
        );

        storage_deposit(TOKEN_CONTRACT_ID, &master_account.account, SAFE_STORAGE_AMOUNT, Some(ORACLE_CONTRACT_ID.to_string()));


        Self {
            contract
        }
    }
}