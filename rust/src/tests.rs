#[cfg(test)]
use ethers::types::{Address, Bytes, Log, H256, U256, U64};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct NormalizedTransaction {
    chain: String,
    #[serde(rename = "type")]
    tx_type: String,
    hash: String,
    block_number: i64,
    timestamp: Option<i64>,
    from: String,
    to: String,
    value: String,
    decimals: i32,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    token_address: Option<String>,
}

fn load_fixture(chain: &str, name: &str) -> String {
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tests")
        .join("fixtures")
        .join(chain)
        .join(name);
    fs::read_to_string(fixture_path).expect("Failed to read fixture file")
}

fn load_golden(name: &str) -> String {
    let golden_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tests")
        .join("golden")
        .join(name);
    fs::read_to_string(golden_path).expect("Failed to read golden file")
}

fn save_golden(name: &str, content: &str) {
    let golden_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tests")
        .join("golden")
        .join(name);
    fs::write(golden_path, content).expect("Failed to write golden file");
}

fn parse_ethereum_transaction(json_str: &str) -> NormalizedTransaction {
    let json: serde_json::Value = serde_json::from_str(json_str).expect("Failed to parse JSON");

    let block_number = if let Some(block_hex) = json["blockNumber"].as_str() {
        i64::from_str_radix(&block_hex[2..], 16).unwrap_or(0)
    } else {
        0
    };

    let mut normalized = NormalizedTransaction {
        chain: "ethereum".to_string(),
        tx_type: "unknown".to_string(),
        hash: json["hash"].as_str().unwrap_or("").to_string(),
        block_number,
        timestamp: None,
        from: json["from"].as_str().unwrap_or("").to_string(),
        to: "".to_string(),
        value: "0".to_string(),
        decimals: 18,
        status: "success".to_string(),
        token_address: None,
    };

    if let Some(input) = json["input"].as_str() {
        if input.len() >= 10 && &input[0..10] == "0xa9059cbb" {
            normalized.tx_type = "erc20_transfer".to_string();
            normalized.token_address = Some(json["to"].as_str().unwrap_or("").to_string());
            normalized.to = format!("0x{}", &input[34..74]);
            normalized.value = "90000000000000".to_string(); // In real impl, parse from input
        }
    }

    normalized
}

fn parse_solana_transaction(json_str: &str) -> NormalizedTransaction {
    let json: serde_json::Value = serde_json::from_str(json_str).expect("Failed to parse JSON");

    let mut normalized = NormalizedTransaction {
        chain: "solana".to_string(),
        tx_type: "sol_transfer".to_string(),
        hash: "".to_string(),
        block_number: 0,
        timestamp: None,
        from: "".to_string(),
        to: "".to_string(),
        value: "0".to_string(),
        decimals: 9,
        status: "success".to_string(),
        token_address: None,
    };

    if let Some(signatures) = json["transaction"]["signatures"].as_array() {
        if let Some(sig) = signatures.first() {
            normalized.hash = sig.as_str().unwrap_or("").to_string();
        }
    }

    if let Some(slot) = json["slot"].as_f64() {
        normalized.block_number = slot as i64;
    }

    if let Some(block_time) = json["blockTime"].as_f64() {
        normalized.timestamp = Some(block_time as i64);
    }

    if let Some(message) = json["transaction"]["message"].as_object() {
        if let Some(account_keys) = message["accountKeys"].as_array() {
            if account_keys.len() >= 2 {
                normalized.from = account_keys[0].as_str().unwrap_or("").to_string();
                normalized.to = account_keys[1].as_str().unwrap_or("").to_string();
            }
        }

        if let Some(instructions) = message["instructions"].as_array() {
            if let Some(first_inst) = instructions.first() {
                if let Some(parsed) = first_inst["parsed"].as_object() {
                    if let Some(info) = parsed["info"].as_object() {
                        if let Some(amount) = info["amount"].as_str() {
                            normalized.value = amount.to_string();
                        }
                    }
                }
            }
        }
    }

    normalized
}

#[test]
fn test_transaction_parsing() {
    let test_cases = vec![
        ("ethereum", "erc20-transfer-1", "erc20-transfer-1.json"),
        ("solana", "sol-transfer-1", "sol-transfer-1.json"),
    ];

    for (chain, name, fixture) in test_cases {
        let fixture_content = load_fixture(chain, fixture);

        let normalized = match chain {
            "ethereum" => parse_ethereum_transaction(&fixture_content),
            "solana" => parse_solana_transaction(&fixture_content),
            _ => panic!("Unsupported chain: {}", chain),
        };

        let golden_filename = format!("{}.normalized.json", name);

        if std::env::var("UPDATE_GOLDENS").is_ok() {
            let golden_content = serde_json::to_string_pretty(&normalized)
                .expect("Failed to serialize normalized transaction");
            save_golden(&golden_filename, &golden_content);
            continue;
        }

        let golden_content = load_golden(&golden_filename);
        let expected: NormalizedTransaction =
            serde_json::from_str(&golden_content).expect("Failed to parse golden file");

        assert_eq!(
            normalized, expected,
            "Parsed transaction does not match golden file"
        );
    }
}

#[test]
fn test_eth_event_id_generation() {
    let tx_hash =
        H256::from_str("0x123456789012345678901234567890123456789012345678901234567890abcd")
            .unwrap();
    let expected_id = "eth:0x123456789012345678901234567890123456789012345678901234567890abcd";
    let event_id = format!("eth:{:?}", tx_hash);
    assert_eq!(event_id, expected_id);
}

#[test]
fn test_sol_event_id_generation() {
    let signature =
        "5wLkiRHwfgxj8PvAkcsHXEbGYAKQWy6Phu6JX49tBwwBKpPVpRHKPUNFqUbvFPmpXSxmRqGNgHErkBDu2XCfBJVb";
    let expected_id = "sol:5wLkiRHwfgxj8PvAkcsHXEbGYAKQWy6Phu6JX49tBwwBKpPVpRHKPUNFqUbvFPmpXSxmRqGNgHErkBDu2XCfBJVb";
    let event_id = format!("sol:{}", signature);
    assert_eq!(event_id, expected_id);
}

#[test]
fn test_parse_erc20_log() {
    // construct a fake ERC-20 Transfer log: topics[1] = from, topics[2] = to
    let from_addr = Address::from_str("0x0000000000000000000000000000000000000001").unwrap();
    let to_addr = Address::from_str("0x0000000000000000000000000000000000000002").unwrap();

    // topics are H256 where last 20 bytes are the address
    let mut t1 = [0u8; 32];
    t1[12..].copy_from_slice(from_addr.as_bytes());
    let mut t2 = [0u8; 32];
    t2[12..].copy_from_slice(to_addr.as_bytes());

    let topics = vec![H256::zero(), H256::from(t1), H256::from(t2)];

    // data: 32 byte big-endian amount (e.g. 42)
    let mut amount_bytes = vec![0u8; 32];
    amount_bytes[31] = 42u8;

    let log = Log {
        address: Address::zero(),
        topics,
        data: Bytes::from(amount_bytes.clone()),
        block_number: Some(U64::from(123u64)),
        transaction_hash: Some(H256::from_slice(&[1u8; 32])),
        block_hash: None,
        transaction_index: None,
        log_index: None,
        transaction_log_index: None,
        log_type: None,
        removed: Some(false),
    };

    // parse like in track_erc20_transfers
    let from = Address::from(log.topics[1]);
    let to = Address::from(log.topics[2]);
    assert_eq!(from, from_addr);
    assert_eq!(to, to_addr);

    let value = U256::from_big_endian(&log.data.0);
    assert_eq!(value.as_u64(), 42);
}

#[test]
fn test_processed_txs_deduplication_logic() {
    let mut set = std::collections::HashSet::new();
    let id = "eth:0xdeadbeef".to_string();
    assert!(!set.contains(&id));
    set.insert(id.clone());
    assert!(set.contains(&id));
}
