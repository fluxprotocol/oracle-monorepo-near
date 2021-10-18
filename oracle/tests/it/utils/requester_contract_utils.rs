use crate::utils::*;
pub struct RequesterContractUtils {
    pub contract: ContractAccount<RequesterContract>,
}

impl RequesterContractUtils {
    pub fn new(master_account: &TestAccount) -> Self {
        // deploy token
        let contract = deploy!(
            // Contract Proxy
            contract: RequesterContract,
            // Contract account id
            contract_id: REQUESTER_CONTRACT_ID,
            // Bytes of contract
            bytes: &REQUESTER_CONTRACT_WASM_BYTES,
            // User deploying the contract,
            signer_account: master_account.account,
            deposit: to_yocto("1000"),
            // init method
            init_method: new(
                ORACLE_CONTRACT_ID.to_string(),
                TOKEN_CONTRACT_ID.to_string(),
                None
            )
        );

        storage_deposit(
            TOKEN_CONTRACT_ID,
            &master_account.account,
            SAFE_STORAGE_AMOUNT,
            Some(REQUESTER_CONTRACT_ID.to_string()),
        );
        storage_deposit(
            ORACLE_CONTRACT_ID,
            &master_account.account,
            5140000000000000000000,
            Some(REQUESTER_CONTRACT_ID.to_string()),
        );

        Self { contract }
    }
}
