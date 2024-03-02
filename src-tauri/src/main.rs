// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
pub mod key_tree;
pub mod stylus;
pub mod sol_utils;
use ethers::{addressbook::Contract, signers::Wallet};
use serde::{Deserialize, Serialize};
use serde_json::{to_writer_pretty, Value};
use sol_utils::SolcOutput;
use sqlx::SqlitePool;
use std::{
    collections::BTreeMap,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    sync::Mutex,
};
use std::{process::Command};
use vyper_rs::vyper::{Evm, Vyper};
pub mod db;
use db::*;
use key_tree::{create_key, get_key_by_name, list_keys, load_keys_to_state, AppState};
use stylus::{stylus_deploy_contract, stylus_estimate_gas};
use tabled::{settings::Style, Table};
use tabled::{Table, settings::Style};
use ethers::core::rand::thread_rng;
use ethers::solc::{Project , ProjectPathsConfig};
use std::str;
#[derive(Serialize, Deserialize)]
struct ContractWalletData {
    abi: Value,
    initcode: String,
}

#[derive(Serialize, Deserialize)]
struct Config {
    provider: String,
    etherscan_api: String,
}

impl ContractWalletData {
    fn new(abi: Value, initcode: String) -> ContractWalletData {
        Self { abi, initcode }
    }
}

#[tauri::command]
async fn fetch_data(path: String) -> Result<ContractWalletData, String> {
    let cpath: &Path = &Path::new(&path);
    let mut contract = Vyper::new(cpath);
    contract.compile().map_err(|e| e.to_string())?;
    contract.gen_abi().map_err(|e| e.to_string())?;
    let abifile = File::open(&contract.abi).map_err(|e| e.to_string())?;
    let reader = BufReader::new(abifile);
    let abifile_json: Value = serde_json::from_reader(reader).map_err(|e| e.to_string())?;
    //println!("{:?}", contract.bytecode.clone().unwrap());
    println!("Back to TS!");
    Ok(ContractWalletData::new(
        abifile_json,
        contract.bytecode.unwrap(),
    ))
}
#[tauri::command]
async fn compile_version(path: String, version: String) -> Result<ContractWalletData, String> {
    let ver: Evm = match &version.as_str() {
        &"Shanghai" => Evm::Shanghai,
        &"Paris" => Evm::Paris,
        &"Berlin" => Evm::Berlin,
        &"Istanbul" => Evm::Istanbul,
        &"Cancun" => Evm::Cancun,
        _ => Evm::Shanghai,
    };
    let cpath: &Path = &Path::new(&path);
    let mut contract = Vyper::new(cpath);
    contract.compile_ver(&ver).map_err(|e| e.to_string())?;
    contract.gen_abi().map_err(|e| e.to_string())?;
    let abifile = File::open(&contract.abi).map_err(|e| e.to_string())?;
    let reader = BufReader::new(abifile);
    let abifile_json: Value = serde_json::from_reader(reader).map_err(|e| e.to_string())?;
    Ok(ContractWalletData::new(
        abifile_json,
        contract.bytecode.unwrap(),
    ))
}

#[tauri::command]
async fn get_keys(key_path: String) -> Result<Value, String> {
    let keyfile = File::open(PathBuf::from(&key_path)).map_err(|e| e.to_string())?;
    let reader = BufReader::new(keyfile);
    let keystore_json: Value = serde_json::from_reader(reader).map_err(|e| e.to_string())?;
    Ok(keystore_json)
}

#[tauri::command]
async fn set_config(provider: String, etherscan_api: String) -> Result<Config, String> {
    let config_path: PathBuf = PathBuf::from("./vyper_deployer_config.json");
    let conf: Config = Config {
        provider,
        etherscan_api,
    };
    let file: File = File::create(config_path).map_err(|e| e.to_string())?;
    to_writer_pretty(file, &conf).map_err(|e| e.to_string())?;
    Ok(conf)
}

#[tauri::command]
async fn get_config() -> Result<Config, String> {
    let file: File = File::open("./vyper_deployer_config.json").map_err(|e| e.to_string())?;
    let reader: BufReader<File> = BufReader::new(file);
    let conf: Config = serde_json::from_reader(reader).map_err(|e| e.to_string())?;
    Ok(conf)
}

#[tauri::command]
async fn db_write(deployment_data: Deployment) -> Result<(), String> {
    let db: &sqlx::Pool<sqlx::Sqlite> = DB_POOL.get().unwrap();
    let name = PathBuf::from(&deployment_data.sc_name)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let query_result = sqlx::query_as!(
        Deployment,
        "INSERT INTO deployments VALUES ($1, $2, $3, $4, $5, $6, $7)",
        name,
        deployment_data.deployer_address,
        deployment_data.deploy_date,
        deployment_data.sc_address,
        deployment_data.network,
        deployment_data.fee,
        deployment_data.verified,
    )
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;
    println!("{query_result:?}");
    Ok(())
}

#[tauri::command]
async fn db_read() -> Result<Vec<Deployment>, String> {
    let db: &sqlx::Pool<sqlx::Sqlite> = DB_POOL.get().unwrap();
    let query: Vec<Deployment> =
        sqlx::query_as!(Deployment, "SELECT * FROM deployments ORDER BY rowid DESC")
            .fetch_all(db)
            .await
            .map_err(|e| e.to_string())?;
    let mut table = Table::new(&query);
    table.with(Style::psql());
    println!("\n{table}");
    Ok(query)
}

#[tauri::command]
fn generate_keystore(path: String, password: String, name: String) -> Result<(), String> {
    Wallet::new_keystore(path, &mut thread_rng(), password, Some(&name)).map_err(|e| e.to_string())?; 
    println!("Success, wallet created!");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    Database::init().await?;
    let pool = SqlitePool::connect(DB_URL).await?;
    sqlx::migrate!("../migrations").run(&pool).await?;
    DB_POOL.set(pool).unwrap();
    load_keys_to_state().await.unwrap();
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            fetch_data,
            set_config,
            get_config,
            get_keys,
            compile_version,
            db_read,
            db_write,
            list_keys,
            create_key,
            stylus_deploy_contract,
            stylus_estimate_gas
            generate_keystore
        ])
        .manage(AppState {
            tree: Mutex::new(BTreeMap::new()),
        })
        .run(tauri::generate_context!())?;
    Ok(())
}



fn compile_solidity(file_path: &str, output_path: &str) -> std::io::Result<()> {
    let solc_path = "/opt/homebrew/bin/solc";

    let output = Command::new(solc_path)
        .args([
            "--combined-json", "abi,bin,metadata",
            "--overwrite",
            file_path,
            "-o", output_path,
        ])
        .output()?;

    if !output.status.success() {
        let e = String::from_utf8_lossy(&output.stderr);
        panic!("Command executed with failing error code: {}", e);
    }

    // Convert the output to a string
    let output_str = String::from_utf8_lossy(&output.stdout);

    // Parse the JSON output
    let parsed: SolcOutput = serde_json::from_str(&output_str).expect("Failed to parse JSON");

    // Example of how to access a contract's ABI and bin. Adjust according to your needs.
    for (key, contract) in parsed.contracts {
        println!("Contract: {}", key);
        println!("ABI: {}", serde_json::to_string_pretty(&contract.abi).unwrap());
        println!("Bytecode: {}", contract.bin);
    }

    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test] // Changed to synchronous test for simplicity
    fn test_compile_test_refactored() {
        let file_path = "src/soliditylayout/contracts/storage.sol";
        let output_path = "src/soliditylayout/contracts";
        //let file_path = "/Users/protocolw/Public/Rustcodes/Protocoldenver/VyperDeployooor/src-tauri/src/soliditylayout/contracts/storage.sol"; // Update this path
        match compile_solidity(file_path , output_path) {
            Ok(()) => println!("Compilation succeeded."),
            Err(e) => eprintln!("Compilation failed: {}", e),
        }
    }
}

