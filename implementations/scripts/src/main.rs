use std::str::FromStr;

use bitcoin::{
    Address, Amount, EcdsaSighashType, Network, PrivateKey, Script, ScriptBuf, TxIn, TxOut, Txid,
    Witness, absolute::LockTime, key::Secp256k1, secp256k1::SecretKey, sighash::SighashCache,
    transaction::Version,
};
use bitcoincore_rpc::{Auth, Client, RpcApi};
use serde::Deserialize;

fn main() {
    let url = "http://127.0.0.1:18443";
    let network = Network::Regtest;
    let auth = Auth::UserPass("alice".to_string(), "password".to_string());
    let client = Client::new(url, auth).expect("msg: Failed to create client");

    if wallet_exists(&client, "testwallet").expect("msg: ") {
        println!("Wallet already exists!");
        if !is_wallet_loaded(&client, "testwallet").expect("msg: ") {
            client.load_wallet("testwallet").expect("msg: ");
            println!("Wallet loaded successfully!");
        } else {
            println!("Wallet is already loaded.");
        }
    } else {
        client
            .create_wallet("testwallet", None, None, None, None)
            .expect("msg: ");
        println!("Wallet created successfully!");
    }

    let recipient_address = client
        .get_new_address(None, None)
        .expect("msg: Failed to get new address")
        .require_network(network)
        .expect("msg: Failed to check network");

    let secp = Secp256k1::new();

    let fake_tx_id =
        Txid::from_str("0000000000000000000000000000000000000000000000000000000000000000")
            .expect("msg: Failed to create fake txid");
    let _recipient = "325UUecEQuyrTd28Xs2hvAxdAjHM7XzqVF";
    let priv_key_1 = "39dc0a9f0b185a2ee56349691f34716e6e0cda06a7f9707742ac113c4e2317bf";
    let priv_key_2 = "5077ccd9c558b7d04a81920d38aa11b4a9f9de3b23fab45c3ef28039920fdd6d";
    let redeem_script_hex = "5221032ff8c5df0bc00fe1ac2319c3b8070d6d1e04cfbf4fedda499ae7b775185ad53b21039bbc8d24f89e5bc44c5b0d1980d6658316a6b2440023117c3c03a4975b04dd5652ae";

    let secret_key_1 =
        SecretKey::from_slice(&hex::decode(&priv_key_1).expect("msg: Failed to decode hex"))
            .expect("msg: Failed to create secret key");
    let secret_key_2 =
        SecretKey::from_slice(&hex::decode(&priv_key_2).expect("msg: Failed to decode hex"))
            .expect("msg: Failed to create secret key");

    let pk1 = PrivateKey::new(secret_key_1, network);
    let pk2 = PrivateKey::new(secret_key_2, network);
    let pbk1 = pk1.public_key(&secp);
    println!("Public Key 1: {}", pbk1);
    let pbk2 = pk2.public_key(&secp);
    println!("Public Key 2: {}", pbk2);

    let witness_script = Script::from_bytes(redeem_script_hex.as_bytes());

    let p2wsh_address = Address::p2wsh(witness_script, network);
    let redeem_script = p2wsh_address.script_pubkey();

    let p2sh_address = Address::p2sh(&redeem_script, network).expect("msg: ");

    println!("Address: {}", p2sh_address);

    // Tx Input - using a fake txid
    let inputs = TxIn {
        previous_output: bitcoincore_rpc::bitcoin::OutPoint {
            txid: fake_tx_id,
            vout: 0,
        },
        script_sig: ScriptBuf::new(),
        sequence: bitcoin::Sequence(0xFFFFFFFF),
        witness: Witness::new(),
    };

    // Tx Output
    let ouput = TxOut {
        value: Amount::from_btc(0.001).expect("msg: Failed to create amount"),
        script_pubkey: recipient_address.script_pubkey(),
    };

    let mut transaction = bitcoin::Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![inputs],
        output: vec![ouput],
    };

    println!("Unsigned transaction: {:?}", transaction);

    let mut sighash_cache = SighashCache::new(&transaction);
    let sighash = sighash_cache
        .p2wsh_signature_hash(
            0, // input index
            &witness_script,
            Amount::from_btc(0.002).expect("msg: Failed to create input amount"), // Assume input amount
            EcdsaSighashType::All,
        )
        .expect("msg: Failed to calculate sighash");

    println!("Sighash: {}", sighash);
    // Sign with both private keys
    let signature1 = secp.sign_ecdsa(&sighash.into(), &secret_key_1);
    let signature2 = secp.sign_ecdsa(&sighash.into(), &secret_key_2);

    // Create DER-encoded signatures with sighash type
    let mut sig1_der = signature1.serialize_der().to_vec();
    sig1_der.push(EcdsaSighashType::All as u8);

    let mut sig2_der = signature2.serialize_der().to_vec();
    sig2_der.push(EcdsaSighashType::All as u8);

    println!("Signature 1: {}", hex::encode(&sig1_der));
    println!("Signature 2: {}", hex::encode(&sig2_der));

    // Create the witness stack for P2WSH multisig
    // Format: [0] [sig1] [sig2] [witness_script] like P2SH https://learnmeabitcoin.com/technical/script/p2wsh/#scriptpubkey
    let mut witness = Witness::new();
    witness.push(&[]); // OP_0 for multisig bug
    witness.push(&sig1_der);
    witness.push(&sig2_der);
    witness.push(witness_script.as_bytes());

    // reset the witness for the input
    transaction.input[0].witness = witness;

    // Create script_sig for P2SH-P2WSH (just the P2WSH script)
    transaction.input[0].script_sig =
        ScriptBuf::from_bytes(p2wsh_address.script_pubkey().to_bytes());

    println!("Final signed transaction: {:?}", transaction);
    println!(
        "Transaction hex: {}",
        bitcoin::consensus::encode::serialize_hex(&transaction)
    );

    println!("\n=== Transaction Analysis ===");
    println!("Inputs: {}", transaction.input.len());
    println!("Outputs: {}", transaction.output.len());
    println!("Input 0:");
    println!(
        "  - Outpoint: {}:{}",
        transaction.input[0].previous_output.txid, transaction.input[0].previous_output.vout
    );
    println!("  - Sequence: 0x{:x}", transaction.input[0].sequence.0);
    println!(
        "  - Script sig length: {}",
        transaction.input[0].script_sig.len()
    );
    println!("  - Witness items: {}", transaction.input[0].witness.len());
    println!("Output 0:");
    println!("  - Value: {} BTC", transaction.output[0].value.to_btc());
    println!(
        "  - Address: {}",
        Address::from_script(&transaction.output[0].script_pubkey, network).unwrap()
    );
    println!("Locktime: {}", transaction.lock_time);

    println!("\nBitcoin P2SH-P2WSH multisig transaction created successfully!");
}

fn wallet_exists(client: &Client, name: &str) -> bitcoincore_rpc::Result<bool> {
    #[derive(Deserialize)]
    struct Name {
        name: String,
    }
    #[derive(Deserialize)]
    struct CallResult {
        wallets: Vec<Name>,
    }
    let res: CallResult = client.call("listwalletdir", &[])?;

    Ok(res.wallets.iter().any(|w| w.name == name))
}

fn is_wallet_loaded(client: &Client, name: &str) -> bitcoincore_rpc::Result<bool> {
    let loaded_wallets: Vec<String> = client.list_wallets()?;
    Ok(loaded_wallets.iter().any(|w| w == name))
}
