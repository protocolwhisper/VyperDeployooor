use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct SolcOutput {
    pub contracts: std::collections::HashMap<String, Contract>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Contract {
    pub abi: serde_json::Value, // You can also define a more detailed structure for the ABI
    pub bin: String,
}
