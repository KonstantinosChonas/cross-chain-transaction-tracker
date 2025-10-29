use anyhow::{anyhow, Result};
use serde_json::Value;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

#[derive(Debug, PartialEq)]
#[allow(dead_code)]
pub struct ParsedTransfer {
    pub from: Pubkey,
    pub to: Pubkey,
    pub amount: u64,
}

/// Extract an SPL token transfer from parsed transaction JSON.
/// Returns None if this is not a token transfer or is malformed.
#[allow(dead_code)]
pub fn parse_spl_transfer(tx: &Value) -> Option<ParsedTransfer> {
    // Token transfers have instruction data in message.instructions
    let instructions = tx.get("message")?.get("instructions")?.as_array()?;

    for ix in instructions {
        // Look for token program
        let program_id = ix.get("programId")?.as_str()?;
        if program_id != "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA" {
            continue;
        }

        // Check if it's a transfer instruction
        if ix.get("data")?.as_str()?.starts_with("3") {
            // Transfer instruction
            let accounts = ix.get("accounts")?.as_array()?;
            if accounts.len() < 3 {
                continue;
            }

            // Get the from and to accounts
            let from = Pubkey::from_str(accounts[0].as_str()?).ok()?;
            let to = Pubkey::from_str(accounts[1].as_str()?).ok()?;

            // Parse amount from instruction data
            let data = ix.get("data")?.as_str()?;
            // Skip the '3' prefix and ensure we have exactly 16 hex digits
            let hex_amount = &data[1..];
            if hex_amount.len() != 16 {
                return None;
            }
            let amount = u64::from_str_radix(hex_amount, 16).ok()?;

            return Some(ParsedTransfer { from, to, amount });
        }
    }
    None
}

/// Validate a transaction has all required fields and decode it
#[allow(dead_code)]
pub fn validate_and_decode_tx(tx: &Value) -> Result<Value> {
    // Required top-level fields
    if tx.get("message").is_none() {
        return Err(anyhow!("Missing message field"));
    }

    let msg = tx.get("message").unwrap();

    // Required message fields
    if msg.get("accountKeys").is_none() {
        return Err(anyhow!("Missing accountKeys field"));
    }

    Ok(tx.clone())
}

/// A small helper that checks whether a parsed Solana transaction JSON
/// contains the watched address among its account keys.
///
/// This is intentionally tolerant: it looks for `message.accountKeys` if present
/// and compares the strings; otherwise returns false.
#[allow(dead_code)]
pub fn parsed_tx_touches_watched(parsed: &Value, watched: &Pubkey) -> bool {
    if let Some(message) = parsed.get("message") {
        if let Some(account_keys) = message.get("accountKeys") {
            if let Some(arr) = account_keys.as_array() {
                let ws = watched.to_string();
                for v in arr.iter() {
                    if let Some(s) = v.as_str() {
                        if s == ws {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

    #[test]
    fn test_parsed_tx_touches_watched_true() {
        let watched = Pubkey::from_str("7xkZG8s8pJ1kG9gA4q3j5Rm4PpG7mVq79k6h4n8P1yqT").unwrap();
        let parsed = json!({
            "message": {
                "accountKeys": [
                    "11111111111111111111111111111111",
                    watched.to_string(),
                    "AnotherPubkey1111111111111111111"
                ]
            }
        });

        assert!(parsed_tx_touches_watched(&parsed, &watched));
    }

    #[test]
    fn test_parsed_tx_touches_watched_false() {
        let watched = Pubkey::from_str("BPFLoader1111111111111111111111111111111111").unwrap();
        let parsed = json!({
            "message": {
                "accountKeys": [
                    "11111111111111111111111111111111",
                    "SomeOther11111111111111111111111111111"
                ]
            }
        });

        assert!(!parsed_tx_touches_watched(&parsed, &watched));
    }

    #[test]
    fn test_parse_spl_transfer_valid() {
        let from = Pubkey::new_unique();
        let to = Pubkey::new_unique();
        let amount = 1000u64;

        // Hex encoded amount prefixed with '3' for transfer instruction
        let data = format!("3{:016x}", amount);

        let tx = json!({
            "message": {
                "instructions": [{
                    "programId": TOKEN_PROGRAM_ID,
                    "accounts": [
                        from.to_string(),
                        to.to_string(),
                        "SomeTokenAccount111111111111111111111111111"
                    ],
                    "data": data
                }]
            }
        });

        let transfer = parse_spl_transfer(&tx).unwrap();
        assert_eq!(transfer.from, from);
        assert_eq!(transfer.to, to);
        assert_eq!(transfer.amount, amount);
    }

    #[test]
    fn test_parse_spl_transfer_invalid_program() {
        let tx = json!({
            "message": {
                "instructions": [{
                    "programId": "WrongProgram1111111111111111111111111111111",
                    "accounts": [
                        "From11111111111111111111111111111111111111111",
                        "To111111111111111111111111111111111111111111",
                    ],
                    "data": "3000000000000003e8"
                }]
            }
        });

        assert!(parse_spl_transfer(&tx).is_none());
    }

    #[test]
    fn test_parse_spl_transfer_missing_accounts() {
        let tx = json!({
            "message": {
                "instructions": [{
                    "programId": TOKEN_PROGRAM_ID,
                    "accounts": [
                        "From11111111111111111111111111111111111111111"
                    ],
                    "data": "3000000000000003e8"
                }]
            }
        });

        assert!(parse_spl_transfer(&tx).is_none());
    }

    #[test]
    fn test_parse_spl_transfer_invalid_amount() {
        let tx = json!({
            "message": {
                "instructions": [{
                    "programId": TOKEN_PROGRAM_ID,
                    "accounts": [
                        "From11111111111111111111111111111111111111111",
                        "To111111111111111111111111111111111111111111",
                        "Token11111111111111111111111111111111111111"
                    ],
                    "data": "3NOT_HEX_NUMBER"
                }]
            }
        });

        assert!(parse_spl_transfer(&tx).is_none());
    }

    #[test]
    fn test_validate_tx_missing_message() {
        let tx = json!({
            "signatures": []
        });

        let result = validate_and_decode_tx(&tx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Missing message"));
    }

    #[test]
    fn test_validate_tx_missing_account_keys() {
        let tx = json!({
            "message": {
                "instructions": []
            }
        });

        let result = validate_and_decode_tx(&tx);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Missing accountKeys"));
    }

    #[test]
    fn test_validate_tx_valid() {
        let tx = json!({
            "message": {
                "accountKeys": [],
                "instructions": []
            }
        });

        let result = validate_and_decode_tx(&tx);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_spl_transfer_max_amount() {
        let from = Pubkey::new_unique();
        let to = Pubkey::new_unique();
        let amount = u64::MAX;

        // Create instruction data for max amount: 3 prefix + FFFFFFFFFFFFFFFF
        let data = "3FFFFFFFFFFFFFFFF".to_string();
        let tx = json!({
            "message": {
                "instructions": [{
                    "programId": TOKEN_PROGRAM_ID,
                    "accounts": [
                        from.to_string(),
                        to.to_string(),
                        "SomeTokenAccount111111111111111111111111111"
                    ],
                    "data": data
                }]
            }
        });
        let transfer = parse_spl_transfer(&tx).unwrap();
        assert_eq!(transfer.amount, amount);
    }

    #[test]
    fn test_parse_spl_transfer_empty_instructions() {
        let tx = json!({
            "message": {
                "instructions": []
            }
        });

        assert!(parse_spl_transfer(&tx).is_none());
    }

    #[test]
    fn test_parse_spl_transfer_malformed_pubkey() {
        let tx = json!({
            "message": {
                "instructions": [{
                    "programId": TOKEN_PROGRAM_ID,
                    "accounts": [
                        "NotAValidPubkey",
                        "To111111111111111111111111111111111111111111",
                        "Token11111111111111111111111111111111111111"
                    ],
                    "data": "3000000000000003e8"
                }]
            }
        });

        assert!(parse_spl_transfer(&tx).is_none());
    }
}
