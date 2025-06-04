use std::{path::Path, str::FromStr};

use bitcoincore_rpc::bitcoin::{
    Address, Amount, BlockHash, ScriptBuf, Sequence, Target, Transaction, TxIn, TxMerkleNode,
    TxOut, Txid, Witness, Wtxid,
    absolute::LockTime,
    block::{Header, Version},
    consensus::{self, Decodable},
    hashes::{
        Hash as OtherHash,
        sha256d::{self, Hash},
    },
    merkle_tree::calculate_root,
    transaction::Version as TxVersion,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct MempoolTransaction {
    txid: String,
    version: u32,
    locktime: u32,
    vin: Vec<Vin>,
    vout: Vec<Vout>,
    size: u32,
    weight: u32,
    fee: u64,
    #[serde(default)]
    status: Option<Status>,
    #[serde(default)]
    hex: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Vin {
    txid: String,
    vout: u32,
    #[serde(default)]
    prevout: Option<Prevout>,
    #[serde(default)]
    scriptsig: String,
    #[serde(default)]
    scriptsig_asm: String,
    #[serde(default)]
    witness: Vec<String>,
    #[serde(default)]
    is_coinbase: bool,
    sequence: u64,
}

#[derive(Debug, Deserialize)]
struct Prevout {
    scriptpubkey: String,
    scriptpubkey_asm: String,
    scriptpubkey_type: String,
    #[serde(default)]
    scriptpubkey_address: Option<String>,
    value: u64,
}

#[derive(Debug, Deserialize)]
struct Vout {
    scriptpubkey: String,
    scriptpubkey_asm: String,
    scriptpubkey_type: String,
    #[serde(default)]
    scriptpubkey_address: Option<String>,
    value: u64,
}

#[derive(Debug, Deserialize)]
struct Status {
    confirmed: bool,
    #[serde(default)]
    block_height: Option<u32>,
    #[serde(default)]
    block_hash: Option<String>,
    #[serde(default)]
    block_time: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ValidTransactions {
    id: String,
    hex: String,
    weight: u32,
    fee: u64,
}

const DIFFICULTY_TARGET: &'static str =
    "0000ffff00000000000000000000000000000000000000000000000000000000";

const MAX_BLOCK_WEIGHT: u32 = 4_000_000;

fn main() {
    let valid_txs = load_txs();
    let miner_address = Address::from_str("tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx")
        .expect("msg: Invalid miner address")
        .assume_checked();
    let previous_hash =
        BlockHash::from_str("0000000000000000000c6f8b1d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e")
            .expect("msg: Invalid previous block hash");
    let (header, txs) = mine_transaction_block(valid_txs, miner_address, previous_hash)
        .expect("msg: Failed to mine transaction block");

    println!("{}", hex::encode(consensus::serialize(&header)));
    println!("{}", hex::encode(consensus::serialize(&txs[0])));

    for tx in &txs {
        println!("{}", tx.compute_txid());
    }
}

// Load valid txs
fn load_txs() -> Vec<ValidTransactions> {
    let txs_path = Path::new("mempool/");
    if !txs_path.exists() {
        panic!("msg: Mempool directory does not exist");
    }
    let mempool_json_path = txs_path.join("mempool.json");
    if !mempool_json_path.exists() {
        panic!("msg: Mempool JSON file does not exist");
    }
    let file = std::fs::File::open(mempool_json_path).expect("msg: Failed to open file");
    let reader = std::io::BufReader::new(file);
    let txs: Vec<String> = serde_json::from_reader(reader).expect("msg: Failed to parse JSON");

    let mut valid_transactions = Vec::new();

    for tx in txs.iter() {
        let tx_path = txs_path.join(format!("{}.json", tx));
        if !tx_path.exists() {
            println!("msg: Transaction file does not exist: {}", tx);
            continue;
        }

        match std::fs::File::open(&tx_path) {
            Ok(tx_file) => match serde_json::from_reader::<_, MempoolTransaction>(tx_file) {
                Ok(tx_data) => {
                    if tx_data.hex.is_some() && !tx_data.txid.is_empty() {
                        let hex = tx_data.hex.clone().unwrap();
                        let valid_tx = ValidTransactions {
                            id: tx_data.txid,
                            hex,
                            weight: tx_data.weight,
                            fee: tx_data.fee,
                        };
                        valid_transactions.push(valid_tx);
                    }
                }
                Err(e) => {
                    println!("msg: Failed to parse transaction file {}: {}", tx, e);
                    continue;
                }
            },
            Err(e) => {
                println!("msg: Failed to open transaction file {}: {}", tx, e);
                continue;
            }
        }
    }

    println!(
        "msg: Successfully loaded {} valid transactions",
        valid_transactions.len()
    );

    valid_transactions
}

// Create coinbase tx
fn create_coinbase_tx(
    miner_address: Address,
    witness_commitment: Option<Vec<u8>>,
) -> Result<Transaction, String> {
    let fake_tx_id =
        Txid::from_str("0000000000000000000000000000000000000000000000000000000000000000")
            .expect("msg: Failed to create fake txid");
    let input = TxIn {
        previous_output: bitcoincore_rpc::bitcoin::OutPoint {
            txid: fake_tx_id,
            vout: 0xffffffff,
        },
        script_sig: ScriptBuf::new(),
        sequence: Sequence(0xffffffff),
        witness: Witness::new(),
    };

    let mut outputs = Vec::new();

    let block_reward = Amount::from_int_btc(50);
    let total_reward = block_reward + Amount::from_sat(1000);

    let output = TxOut {
        value: total_reward,
        script_pubkey: miner_address.script_pubkey(),
    };

    outputs.push(output);

    if let Some(commitment) = witness_commitment {
        let mut witness_script = vec![0x6a, 0x24, 0xaa, 0x21, 0xa9, 0xed];
        witness_script.extend_from_slice(&commitment);

        outputs.push(TxOut {
            value: Amount::ZERO,
            script_pubkey: ScriptBuf::from(witness_script),
        });
    }

    let tx = Transaction {
        version: TxVersion::TWO,
        lock_time: LockTime::ZERO,
        input: vec![input],
        output: outputs,
    };

    Ok(tx)
}

fn calculate_merkle_root(txids: Vec<Txid>) -> TxMerkleNode {
    if txids.is_empty() {
        return TxMerkleNode::all_zeros();
    }

    let hashes: Vec<sha256d::Hash> = txids.iter().map(|txid| txid.to_raw_hash()).collect();
    let merkle_root = calculate_root(hashes.into_iter()).unwrap();

    TxMerkleNode::from_raw_hash(merkle_root)
}

fn select_transactions(
    valid_txs: Vec<ValidTransactions>,
    max_weight: u32,
) -> Vec<ValidTransactions> {
    let mut selected = Vec::new();
    let mut total_weight = 0u32;

    let mut sorted_txs = valid_txs;
    sorted_txs.sort_by(|a, b| {
        let fee_per_weight_a = a.fee as f64 / a.weight as f64;
        let fee_per_weight_b = b.fee as f64 / b.weight as f64;
        fee_per_weight_b.partial_cmp(&fee_per_weight_a).unwrap()
    });

    for tx in sorted_txs {
        if total_weight + tx.weight <= max_weight {
            total_weight += tx.weight;
            selected.push(tx);
        }
    }

    selected
}

fn create_block_header(
    previous_hash: Hash,
    merkle_root: Hash,
    timestamp: u32,
    nonce: u32,
) -> Result<Header, String> {
    let target_bytes = hex::decode(DIFFICULTY_TARGET).expect("Invalid difficulty target hex");
    let mut target_array = [0u8; 32];
    target_array.copy_from_slice(&target_bytes);
    let target = Target::from_be_bytes(target_array);

    let block_header = Header {
        version: Version::TWO,
        prev_blockhash: BlockHash::from_raw_hash(previous_hash),
        merkle_root: TxMerkleNode::from_raw_hash(merkle_root),
        time: timestamp,
        nonce,
        bits: target.to_compact_lossy(),
    };

    Ok(block_header)
}

fn hash_block_header(header: &Header) -> BlockHash {
    let serialized = consensus::serialize(header);
    BlockHash::hash(&serialized)
}

// Compute witness transaction IDs (wtxids) for witness commitment
// Compute the witness commitment
fn calculate_witness_commitment(wtxids: &Vec<Wtxid>) -> Vec<u8> {
    // This is a just simplified version
    let mut commitment = Vec::new();
    for wtxid in wtxids {
        commitment.extend_from_slice(&wtxid.to_byte_array());
    }
    commitment
}

fn mine_block(mut header: Header) -> Result<Header, String> {
    let target = Target::from_compact(header.bits);
    for nonce in 0..u32::MAX {
        header.nonce = nonce;

        let hash = hash_block_header(&header);

        let hash_as_u256 = Target::from_le_bytes(hash.to_byte_array());

        if hash_as_u256 <= target {
            println!("Block mined! Nonce: {}, Hash: {}", nonce, hash);
            return Ok(header);
        }

        if nonce % 100000 == 0 {
            println!("Mining... nonce: {}", nonce);
        }
    }

    Err("Failed to find valid nonce".to_string())
}

fn mine_transaction_block(
    valid_transactions: Vec<ValidTransactions>,
    miner_address: Address,
    previous_hash: BlockHash,
) -> Result<(Header, Vec<Transaction>), String> {
    let temp_coinbase_tx = create_coinbase_tx(miner_address.clone(), None)?;

    let mut wtxids = vec![temp_coinbase_tx.compute_wtxid()];
    let witness_commitment = calculate_witness_commitment(&wtxids);

    let coinbase_tx = create_coinbase_tx(miner_address, Some(witness_commitment))
        .map_err(|e| format!("Failed to create coinbase transaction: {}", e))?;
    let mut block_transactions = vec![coinbase_tx.clone()];
    let coinbase_weight = coinbase_tx.weight().to_wu() as u32;
    let available_weight = MAX_BLOCK_WEIGHT - coinbase_weight;

    let selected_txs = select_transactions(valid_transactions, available_weight);

    for tx_data in &selected_txs {
        match hex::decode(&tx_data.hex) {
            Ok(tx_bytes) => match Transaction::consensus_decode(&mut tx_bytes.as_slice()) {
                Ok(tx) => {
                    wtxids.push(tx.clone().into());
                    block_transactions.push(tx);
                }
                Err(e) => println!("Failed to decode transaction {}: {}", tx_data.id, e),
            },
            Err(e) => println!("Failed to decode hex for {}: {}", tx_data.id, e),
        }
    }

    let txids: Vec<Txid> = block_transactions
        .iter()
        .map(|tx| tx.compute_txid())
        .collect();
    let merkle_root = calculate_merkle_root(txids);

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32;

    let header = create_block_header(
        previous_hash.to_raw_hash(),
        merkle_root.to_raw_hash(),
        timestamp,
        0,
    )?;

    let mined_header = mine_block(header)?;

    println!(
        "Successfully mined block with {} transactions",
        block_transactions.len()
    );
    println!(
        "Total fees collected: {} sats",
        selected_txs.iter().map(|tx| tx.fee).sum::<u64>()
    );

    Ok((mined_header, block_transactions))
}
