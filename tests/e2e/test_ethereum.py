import pytest
import subprocess
import time
import os
import json
import logging
from web3 import Web3
from solcx import compile_standard, install_solc
import requests

logger = logging.getLogger("e2e.ethereum")


@pytest.mark.parametrize("transfer_count", [1, 3], ids=["single", "batch"])
def test_ethereum_e2e(transfer_count):
    """
    Full Ethereum E2E test:
    1. Connect to Anvil
    2. Deploy ERC20 contract
    3. Send token transfer(s) - parameterized for single or batch
    4. Verify Rust logs show normalized event
    5. Verify API returns transaction(s)
    """
    # 1. Connect to Anvil (Ethereum emulator). We expect the anvil service
    # to be started by docker-compose at http://127.0.0.1:8545
    rpc = os.getenv("ANVIL_RPC", "http://127.0.0.1:8545")
    w3 = Web3(Web3.HTTPProvider(rpc))

    # Retry connection to Anvil (it may take a few seconds to be ready)
    connected = False
    for attempt in range(20):
        if w3.is_connected():
            connected = True
            logger.info(f"Connected to Anvil at {rpc}")
            break
        logger.warning(f"Anvil not ready, waiting... (attempt {attempt + 1}/20)")
        time.sleep(2)

    assert connected, f"could not connect to Anvil RPC at {rpc} after 20 attempts"

    accounts = w3.eth.accounts
    assert (
        len(accounts) >= transfer_count + 1
    ), f"need at least {transfer_count + 1} accounts from Anvil"
    deployer = accounts[0]
    recipients = accounts[1 : transfer_count + 1]

    logger.info(f"Using deployer: {deployer}, recipients: {recipients}")

    # 2. Compile and deploy ERC20Mock
    contract_path = os.path.join(os.path.dirname(__file__), "contracts", "ERC20.sol")
    with open(contract_path, "r", encoding="utf-8") as f:
        source = f.read()

    install_solc("0.8.20")
    compiled = compile_standard(
        {
            "language": "Solidity",
            "sources": {"ERC20.sol": {"content": source}},
            "settings": {"outputSelection": {"*": {"*": ["abi", "evm.bytecode"]}}},
        },
        solc_version="0.8.20",
    )

    abi = compiled["contracts"]["ERC20.sol"]["ERC20Mock"]["abi"]
    bytecode = compiled["contracts"]["ERC20.sol"]["ERC20Mock"]["evm"]["bytecode"][
        "object"
    ]

    contract = w3.eth.contract(abi=abi, bytecode=bytecode)
    tx_hash = contract.constructor(10**21).transact({"from": deployer})
    tx_receipt = w3.eth.wait_for_transaction_receipt(tx_hash, timeout=30)
    contract_addr = tx_receipt.contractAddress
    logger.info(f"Deployed ERC20 at {contract_addr}")

    token = w3.eth.contract(address=contract_addr, abi=abi)

    # 3. Transfer tokens from deployer to recipient(s)
    transfer_amount = 10**18
    tx_hashes = []

    for idx, recipient in enumerate(recipients):
        tx_hash = token.functions.transfer(recipient, transfer_amount).transact(
            {"from": deployer}
        )
        tx_receipt = w3.eth.wait_for_transaction_receipt(tx_hash, timeout=30)
        hex_tx = tx_receipt.transactionHash.hex()
        tx_hashes.append(hex_tx)
        logger.info(
            f"Sent ERC20 transfer {idx + 1}/{transfer_count}: {hex_tx} to {recipient}"
        )

    # 4. Verify Rust emitted normalized messages
    # Check docker logs for the rust service to find the published event
    logger.info("Checking Rust logs for normalized event...")
    time.sleep(3)  # Give Rust time to process and publish

    rust_logs = subprocess.run(
        ["docker", "logs", "infra-rust-1", "--tail", "200"],
        capture_output=True,
        text=True,
        timeout=10,
    )

    logs_output = rust_logs.stdout + rust_logs.stderr
    logger.info(f"Rust logs sample: {logs_output[-500:]}")

    # Check if all transaction hashes appear in logs
    for hex_tx in tx_hashes:
        assert (
            hex_tx.lower() in logs_output.lower()
        ), f"Transaction {hex_tx} not found in Rust logs"

    # Check if "Published event to Redis" message appears
    assert (
        "Published event to Redis" in logs_output
        or "published event" in logs_output.lower()
    ), "No evidence of event publication in Rust logs"

    logger.info(f"✓ Rust logs show {transfer_count} normalized event(s) were published")

    # 5. Poll the API for the transaction(s)
    api_url = os.getenv("API_URL", "http://127.0.0.1:3000")

    for idx, (recipient, hex_tx) in enumerate(zip(recipients, tx_hashes)):
        wallet_endpoint = f"{api_url}/wallet/{recipient}/transactions"

        found = False
        event_data = None
        for attempt in range(30):
            try:
                resp = requests.get(wallet_endpoint, timeout=2)
                if resp.status_code == 200:
                    data = resp.json()
                    if attempt == 0:
                        logger.info(f"API returned {len(data)} events for {recipient}")
                        if data:
                            logger.info(f"First event: {data[0]}")
                    for ev in data:
                        # Normalize transaction hashes: remove 0x prefix and compare case-insensitively
                        api_tx = ev.get("tx_hash", "").lower().replace("0x", "")
                        test_tx = hex_tx.lower().replace("0x", "")
                        if api_tx == test_tx:
                            found = True
                            event_data = ev
                            break
                    if found:
                        break
            except Exception as e:
                logger.warning(f"API polling attempt {attempt + 1} failed: {e}")
            time.sleep(1)

        assert (
            found
        ), f"Transaction {hex_tx} not found via API at {wallet_endpoint}. Looking for tx_hash={hex_tx}"
        logger.info(
            f"✓ API returned transaction {idx + 1}/{transfer_count}: {json.dumps(event_data, indent=2)}"
        )

        # 6. Validate event structure
        assert event_data["chain"] == "ethereum", "Event chain should be ethereum"
        assert (
            event_data["event_type"] == "erc20_transfer"
        ), "Event type should be erc20_transfer"
        assert (
            event_data["from"].lower() == deployer.lower()
        ), "Event from address mismatch"
        assert (
            event_data["to"].lower() == recipient.lower()
        ), "Event to address mismatch"
        assert event_data["value"] == str(transfer_amount), "Event value mismatch"

    logger.info(f"✓ Ethereum E2E test passed for {transfer_count} transfer(s)")
