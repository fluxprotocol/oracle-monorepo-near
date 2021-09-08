use crate::utils::*;
use oracle::data_request::DataRequestDataType;
use oracle::types::*;
pub fn init_balance() -> u128 {
    to_yocto("100000")
}

pub struct TestAccount {
    pub account: UserAccount
}

impl TestAccount {
    pub fn new(
        master_account: Option<&UserAccount>, 
        account_id: Option<&str>
    ) -> Self {
        match master_account {
            Some(master_account) => {
                let account = master_account.create_user(account_id.expect("expected account id").to_string(), init_balance());
                storage_deposit(TOKEN_CONTRACT_ID, &master_account, SAFE_STORAGE_AMOUNT, Some(account.account_id())); 
                storage_deposit(ORACLE_CONTRACT_ID, &master_account, 46800000000000000000000, Some(account.account_id())); 
                storage_deposit(ORACLE_CONTRACT_ID, &master_account, 46800000000000000000000, Some(REQUESTOR_CONTRACT_ID.to_string())); 
                near_deposit(&account, init_balance() / 2);
                Self {
                    account
                }
            },
            None => Self { account: init_simulator(None) }
        }
    }

    /*** Getters ***/
    pub fn get_token_balance(&self, account_id: Option<String>) -> u128 {
        let account_id = match account_id {
            Some(account_id) => account_id,
            None => self.account.account_id()
        };

        let res: U128 = self.account.view(
            TOKEN_CONTRACT_ID.to_string(),
            "ft_balance_of",
            json!({
                "account_id": account_id
            }).to_string().as_bytes()
        ).unwrap_json();

        res.into()
    }

    pub fn dr_exists(&self, id: u64) -> bool {
        self.account.view(
            ORACLE_CONTRACT_ID.to_string(),
            "dr_exists",
            json!({
                "id": U64(id)
            }).to_string().as_bytes()
        ).unwrap_json()
    }

    pub fn get_outcome(&self, id: u64) -> Option<Outcome> {
        self.account.call(
            REQUESTOR_CONTRACT_ID.to_string(),
            "get_outcome",
            json!({
                "request_id": U64(id)
            }).to_string().as_bytes(),
            DEFAULT_GAS,
            0
        ).unwrap_json()
    }
    
    /*** Setters ***/
    pub fn dr_new(
        &self,
        fee: u128,
        custom_validity_bond: Option<u128>
    ) -> ExecutionResult {

        // Transfer validity bond to to the request interface contract, this way it has balance to pay for the DataRequest creation
        let transfer_res = self.account.call(
            TOKEN_CONTRACT_ID.to_string(), 
            "ft_transfer", 
            json!({
                "receiver_id": REQUESTOR_CONTRACT_ID,
                "amount": U128(custom_validity_bond.unwrap_or(VALIDITY_BOND) + fee),
            }).to_string().as_bytes(),
            DEFAULT_GAS,
            1
        );
        assert!(transfer_res.is_ok(), "ft_transfer_call failed with res: {:?}", transfer_res);
        
        let dr_new_res = self.account.call(
            REQUESTOR_CONTRACT_ID.to_string(), 
            "create_data_request", 
            json!({
                "amount": U128(custom_validity_bond.unwrap_or(VALIDITY_BOND) + fee),
                "payload": NewDataRequestArgs {
                    sources: vec![],
                    tags: vec!["1".to_string()],
                    description: Some("test description".to_string()),
                    outcomes: None,
                    challenge_period: U64(1000),
                    data_type: DataRequestDataType::String,
                    creator: self.account.account_id(),
                }
            }).to_string().as_bytes(),
            DEFAULT_GAS,
            0
        );

        assert!(dr_new_res.is_ok(), "ft_transfer_call failed with res: {:?}", dr_new_res);
        dr_new_res
    }

    pub fn stake(
        &self,
        dr_id: u64, 
        outcome: Outcome,
        amount: u128
    ) -> ExecutionResult {
        let msg = json!({
            "StakeDataRequest": {
                "outcome": outcome,
                "id": U64(dr_id)
            }
        }).to_string();
        let res = self.ft_transfer_call(ORACLE_CONTRACT_ID, amount, msg);
        res.assert_success();
        res
    }

    pub fn finalize(
        &self,
        dr_id: u64
    ) -> ExecutionResult {
        let res = self.account.call(
            ORACLE_CONTRACT_ID.to_string(), 
            "dr_finalize", 
            json!({
                "request_id": U64(dr_id)
            }).to_string().as_bytes(),
            DEFAULT_GAS,
            0
        );

        res.assert_success();
        res
    }

    pub fn dr_final_arbitrator_finalize(
        &self,
        dr_id: u64,
        outcome: Outcome
    ) -> ExecutionResult {
        let res = self.account.call(
            ORACLE_CONTRACT_ID.to_string(), 
            "dr_final_arbitrator_finalize", 
            json!({
                "request_id": U64(dr_id),
                "outcome": outcome
            }).to_string().as_bytes(),
            DEFAULT_GAS,
            1600000000000000000000
        );

        res.assert_success();
        res
    }

    pub fn claim(
        &self,
        dr_id: u64
    ) -> ExecutionResult {
        let res = self.account.call(
            ORACLE_CONTRACT_ID.to_string(), 
            "dr_claim", 
            json!({
                "account_id": self.account.account_id(),
                "request_id": U64(dr_id)
            }).to_string().as_bytes(),
            DEFAULT_GAS,
            1000000000000000000000
        );

        res.assert_success();
        res
    }

    fn ft_transfer_call(
        &self,
        receiver: &str,
        amount: u128,
        msg: String
    ) -> ExecutionResult {        
        let res = self.account.call(
            TOKEN_CONTRACT_ID.to_string(), 
            "ft_transfer_call", 
            json!({
                "receiver_id": receiver,
                "amount": U128(amount),
                "msg": msg,
                "memo": "".to_string()
            }).to_string().as_bytes(),
            DEFAULT_GAS,
            1
        );

        assert!(res.is_ok(), "ft_transfer_call failed with res: {:?}", res);
        res
    }

    pub fn ft_transfer(
        &self,
        receiver: &str,
        amount: u128
    ) -> ExecutionResult {        
        let res = self.account.call(
            TOKEN_CONTRACT_ID.to_string(), 
            "ft_transfer", 
            json!({
                "receiver_id": receiver,
                "amount": U128(amount),
            }).to_string().as_bytes(),
            DEFAULT_GAS,
            1
        );

        assert!(res.is_ok(), "ft_transfer failed with res: {:?}", res);
        res
    }

}
