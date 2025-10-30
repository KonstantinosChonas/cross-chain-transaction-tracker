#!/usr/bin/env python3
import os
import time
import json
import subprocess
import requests
from web3 import Web3
from solcx import compile_source, install_solc
from eth_account import Account
import signal
import sys

# Constants
ANVIL_URL = "http://localhost:8545"
GO_API_URL = "http://localhost:8080"
TEST_PRIVATE_KEY = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"  # Anvil default account
TEST_CONTRACT = """
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract TestToken {
    string public name = "Test Token";
    string public symbol = "TST";
    uint8 public decimals = 18;
    uint256 public totalSupply;
    mapping(address => uint256) public balanceOf;
    
    event Transfer(address indexed from, address indexed to, uint256 value);
    
    constructor() {
        totalSupply = 1000000 * 10**18;
        balanceOf[msg.sender] = totalSupply;
    }
    
    function transfer(address to, uint256 value) public returns (bool) {
        require(balanceOf[msg.sender] >= value, "Insufficient balance");
        balanceOf[msg.sender] -= value;
        balanceOf[to] += value;
        emit Transfer(msg.sender, to, value);
        return true;
    }
}
"""


def wait_for_service(url, max_retries=30, delay=1):
    """Wait for a service to become available"""
    for _ in range(max_retries):
        try:
            requests.get(url)
            return True
        except requests.exceptions.ConnectionError:
            time.sleep(delay)
    return False


def deploy_test_contract(w3, account):
    """Deploy the test ERC20 contract"""
    print("Deploying test contract...")
    print("Compiling contract...")
    # Install and use solc 0.8.x
    install_solc("0.8.20")

    # Compile the contract using py-solc-x
    compiled_sol = compile_source(TEST_CONTRACT, output_values=["abi", "bin"])
    contract_id, contract_interface = compiled_sol.popitem()
    abi = contract_interface["abi"]
    bytecode = contract_interface["bin"]

    # Deploy contract
    contract = w3.eth.contract(abi=abi, bytecode=bytecode)
    tx_hash = contract.constructor().transact({"from": account.address})
    tx_receipt = w3.eth.wait_for_transaction_receipt(tx_hash)

    return w3.eth.contract(address=tx_receipt.contractAddress, abi=abi)


def wait_for_event(api_url, timeout=10):
    """Poll the API for the expected event"""
    start_time = time.time()
    while time.time() - start_time < timeout:
        try:
            resp = requests.get(f"{api_url}/internal/last-received")
            if resp.status_code == 200:
                events = resp.json()
                if events and len(events) > 0:
                    return events[0]
        except requests.exceptions.ConnectionError:
            pass
        time.sleep(0.5)
    return None


def cleanup(procs):
    """Clean up processes and temporary files"""
    for proc in procs:
        if proc and proc.poll() is None:
            proc.terminate()
            proc.wait()

    # No need to cleanup TestToken.sol anymore


def run_event_delivery_test():
    """Core test routine: raises Exception on failure, always cleans up."""
    procs = []
    try:
        # Start test infrastructure
        print("Starting test infrastructure...")
        # Resolve repository root and handle WSL paths
        repo_root = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))

        # If we're in WSL, convert the path
        if os.environ.get("WSL_DISTRO_NAME"):
            # If the path starts with a Windows-style drive letter, convert it
            if repo_root.startswith("/mnt/c/"):
                # Remove duplicate /mnt/c if present
                repo_root = repo_root.replace("/mnt/c/mnt/c/", "/mnt/c/")
            elif ":" in repo_root:  # Windows path
                repo_root = subprocess.check_output(
                    ["wslpath", "-a", repo_root], text=True
                ).strip()
        else:
            # For Windows native Python, just normalize slashes
            repo_root = repo_root.replace("\\", "/")

        start_sh = os.path.join(repo_root, "scripts", "test-start.sh").replace(
            "\\", "/"
        )
        subprocess.run(["bash", start_sh], check=True)

        # Wait for Anvil to be ready
        if not wait_for_service(ANVIL_URL):
            raise Exception("Anvil failed to start")

        # Start Rust listener (test mode)
        print("Starting Rust listener...")
        rust_env = os.environ.copy()
        rust_env.update(
            {
                "TEST_MODE": "true",
                "ETH_RPC_URL": ANVIL_URL,
                "REDIS_URL": "redis://localhost:6379",
            }
        )
        rust_manifest = os.path.join(repo_root, "rust", "Cargo.toml").replace("\\", "/")
        rust_proc = subprocess.Popen(
            ["cargo", "run", "--manifest-path", rust_manifest], env=rust_env
        )
        procs.append(rust_proc)

        # Start Go API (test mode)
        print("Starting Go API...")
        go_env = os.environ.copy()
        go_env.update(
            {
                "TEST_MODE": "true",
                "REDIS_URL": "redis://localhost:6379",
            }
        )
        go_main = os.path.join(repo_root, "go", "cmd", "api", "main.go").replace(
            "\\", "/"
        )
        go_proc = subprocess.Popen(["go", "run", go_main], env=go_env)
        procs.append(go_proc)

        # Wait for API to be ready
        if not wait_for_service(GO_API_URL):
            raise Exception("Go API failed to start")

        # Setup Web3 and account
        w3 = Web3(Web3.HTTPProvider(ANVIL_URL))
        account = Account.from_key(TEST_PRIVATE_KEY)

        # Deploy test contract
        contract = deploy_test_contract(w3, account)
        print(f"Test contract deployed at: {contract.address}")

        # Send test transfer
        watched_address = (
            "0x70997970C51812dc3A010C7d01b50e0d17dc79C8"  # Anvil test account 2
        )
        amount = 100 * 10**18  # 100 tokens

        print(f"Sending test transfer to {watched_address}...")
        tx_hash = contract.functions.transfer(watched_address, amount).transact(
            {"from": account.address}
        )
        w3.eth.wait_for_transaction_receipt(tx_hash)

        # Wait for event to be received by Go service
        print("Waiting for event to be received...")
        event = wait_for_event(GO_API_URL)

        if not event:
            raise Exception("No event received within timeout")

        # Verify event details
        assert event["chain"] == "ethereum"
        assert event["event_type"] == "erc20_transfer"
        assert event["from"].lower() == account.address.lower()
        assert event["to"].lower() == watched_address.lower()
        assert event["token"]["address"].lower() == contract.address.lower()
        assert event["token"]["symbol"] == "TST"

        print("Test passed successfully!")

    except Exception:
        # Re-raise so pytest can fail the test and report nicely
        raise

    finally:
        # Cleanup
        cleanup(procs)
        stop_sh = os.path.join(repo_root, "scripts", "test-stop.sh").replace("\\", "/")
        subprocess.run(["bash", stop_sh])


def main():
    try:
        run_event_delivery_test()
        sys.exit(0)
    except Exception as e:
        print(f"Test failed: {e}")
        sys.exit(1)


if __name__ == "__main__":
    main()


# Pytest entrypoint
def test_event_delivery():
    run_event_delivery_test()
