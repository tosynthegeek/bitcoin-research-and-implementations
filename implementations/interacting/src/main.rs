use bitcoincore_rpc::{Auth, Client, RpcApi, bitcoin::Amount, json::FundRawTransactionOptions};
use serde::Deserialize;
use serde_json::{Value, json};

fn main() -> bitcoincore_rpc::Result<()> {
    println!("Hello, world!");

    let url = "http://127.0.0.1:18443";
    let network = bitcoincore_rpc::bitcoin::Network::Regtest;
    let auth = Auth::UserPass("alice".to_string(), "password".to_string());
    let client = Client::new(url, auth)?;
    let recipient = "bcrt1qvcmqzaqja09kflan6aafqrgjrykl08s0c35p22";

    println!("Client created successfully!");

    if wallet_exists(&client, "testwallet")? {
        println!("Wallet already exists!");
        if !is_wallet_loaded(&client, "testwallet")? {
            client.load_wallet("testwallet")?;
            println!("Wallet loaded successfully!");
        } else {
            println!("Wallet is already loaded.");
        }
    } else {
        client.create_wallet("testwallet", None, None, None, None)?;
        println!("Wallet created successfully!");
    }

    let address = client.get_new_address(None, None)?;
    println!("New address: {:?}", address);

    let balance = client.get_balance(None, None)?;
    println!("Wallet balance: {:?}", balance);

    let address = address.require_network(network).unwrap();

    let _ = client.generate_to_address(40, &address);

    let fee_rate = Amount::from_sat(21 * 1000);
    let op_return_hex = hex::encode("We are all Satoshi!!".as_bytes());
    let _ = "57652061726520616c6c205361746f7368692121";
    println!("Op return: {}", op_return_hex);

    let outs = json!([
        {
            recipient.to_string() : Amount::from_int_btc(100).to_btc()
        },
        {
            "data": op_return_hex
        }
    ]);

    let res: Value = match client.call("createrawtransaction", &[json!([]), outs]) {
        Ok(value) => value,
        Err(e) => {
            println!("Error creating raw transaction: {}", e);
            return Err(e);
        }
    };

    let tx_hex = match res.as_str() {
        Some(hex) => hex.to_string(),
        None => {
            println!("Errorr...");
            return Err(bitcoincore_rpc::Error::UnexpectedStructure);
        }
    };

    println!("Tx hex: {}", tx_hex);

    let funded_tx = client.fund_raw_transaction(
        tx_hex,
        Some(&FundRawTransactionOptions {
            replaceable: Some(false),
            fee_rate: Some(fee_rate),
            ..Default::default()
        }),
        None,
    )?;

    let signed_tx = client.sign_raw_transaction_with_wallet(&funded_tx.hex, None, None)?;

    if !signed_tx.complete {
        println!("Incomplete Tx");
    }

    let decoded_signed_tx = client.decode_raw_transaction(&signed_tx.hex, None)?;

    for vout in decoded_signed_tx.vout {
        let script_pub_key = &vout.script_pub_key;
        if script_pub_key.asm.starts_with("OP_RETURN") {
            println!("Found OP_RETURN output: {:?}", script_pub_key.asm);
        }
    }

    let txid = client.send_raw_transaction(&signed_tx.hex)?;
    println!("Transaction sent with ID: {}", txid);
    let tx_info = client.get_transaction(&txid, None)?;
    println!("Transaction info: {:?}", tx_info);

    Ok(())
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
