use oracle::data_request::DataRequestDataType;
use crate::utils::*;

pub fn init_balance() -> u128 {
    to_yocto("1000")
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

    /*** Setters ***/
    pub fn dr_new(
        &self,
    ) -> ExecutionResult {

        // Transfer validity bond to to the request interface contract, this way it has balance to pay for the DataRequest creation
        let transfer_res = self.account.call(
            TOKEN_CONTRACT_ID.to_string(), 
            "ft_transfer", 
            json!({
                "receiver_id": REQUEST_INTERFACE_CONTRACT_ID,
                "amount": U128(VALIDITY_BOND),
            }).to_string().as_bytes(),
            DEFAULT_GAS,
            1
        );
        assert!(transfer_res.is_ok(), "ft_transfer_call failed with res: {:?}", transfer_res);
        
        let dr_new_res = self.account.call(
            REQUEST_INTERFACE_CONTRACT_ID.to_string(), 
            "create_data_request", 
            json!({
                "amount": U128(100),
                "payload": NewDataRequestArgs {
                    sources: vec![],
                    tags: None,
                    description: Some("test description".to_string()),
                    outcomes: None,
                    challenge_period: U64(1000),
                    settlement_time: U64(10000),
                    target_contract: TARGET_CONTRACT_ID.to_string(),
                    data_type: data_request::DataRequestDataType::String,
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
        outcome: data_request::Outcome,
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
            900000000000000000000
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
            900000000000000000000
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

}
