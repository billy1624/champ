use crate::state::ChampStateArc;
use crate::storage;

use anyhow::Result;
use crypto::{self, signatures::ed25519::verify_signature};
use encoding::account::generate_account_address;
use pog_proto::api::{
    transaction::{Data, TxClaim},
    SignedBlock,
};
use prost::Message;
use std::convert::TryInto;
use storage::Database;
use thiserror::Error;
use tracing::{debug, trace};

#[derive(Error, Debug)]
pub enum Validation {
    #[error("transactions could not be validated")]
    TxValidationError,
    #[error("invalid block height")]
    BlockHeightError,
    #[error("previous block did not match")]
    PreviousBlockError,
    #[error("transaction data not found")]
    TransactionDataNotFound,
    #[error("corresponding send tx not found")]
    SendTxNotFound,
    #[error("attempt to claim non send tx")]
    MissmatchedTx,
    #[error("duplicate transaction")]
    DuplicatedTx,
    #[error("too many transactions")]
    TooManyTransactions,
}

#[derive(Error, Debug)]
#[allow(clippy::enum_variant_names)]
pub enum Node {
    #[error("block data not found")]
    BlockDataNotFound,
    #[error("block not found")]
    BlockNotFound,
    #[error{"tx not found"}]
    TxNotFound,
}

// Validate block
#[allow(dead_code)]
#[tracing::instrument]
pub async fn validate(block: &SignedBlock, state: &ChampStateArc) -> Result<()> {
    debug!("validating a block");

    let data = block.clone().data.ok_or(Node::BlockDataNotFound)?;
    let public_key = &block.public_key;
    let signature = &block.signature;
    let db = &state.db.lock().await;
    let account_id = generate_account_address(public_key.to_vec())?;

    let response = db.get_latest_block_by_account(account_id).await;

    let latest_block = match response {
        Ok(block) => block,
        Err(storage::DatabaseError::NoLastBlock) => return verify_account_genesis_block(),
        _ => return Err(Node::BlockNotFound.into()),
    };

    // TODO: verify block doesn't already exist
    // signature
    verify_signature(&data.encode_to_vec(), public_key, signature)?;
    // height / previous block
    verify_previous_block(block, &latest_block)?;
    // transactions / balance
    verify_transactions(block, &latest_block, state).await?;

    trace!("Block successfully validated. Block={:?}", block);

    Ok(())
}

// TODO: add error handling so validation error go to voting
// Verifies the transactions and balances
async fn verify_transactions(new_block: &SignedBlock, prev_block: &SignedBlock, state: &ChampStateArc) -> Result<()> {
    debug!("verify transactions");
    let db = &state.db.lock().await;
    // go through all tx in the block and do math to see new balance
    // check against block balance
    let new_data = new_block.data.as_ref().ok_or(Node::BlockDataNotFound)?;
    let prev_data = prev_block.data.as_ref().ok_or(Node::BlockDataNotFound)?;

    let mut transaction_ids: Vec<[u8; 32]> = vec![];

    if new_data.transactions.len() > 255 {
        return Err(Validation::TooManyTransactions.into());
    }

    // TODO: Run concurrently
    let mut new_balance: i128 = prev_data.balance as i128;
    for transaction in &new_data.transactions {
        // validate that transaction is not duplicated
        let txid = transaction.get_id(new_block.get_id()?)?;
        if transaction_ids.contains(&txid) {
            return Err(Validation::DuplicatedTx.into());
        }
        transaction_ids.push(txid);

        // calculate the new balance after all transactions are processed
        let tx_type = transaction.data.as_ref().ok_or(Validation::TransactionDataNotFound)?;
        new_balance += match tx_type {
            Data::TxSend(t) => -(t.amount as i128),
            Data::TxCollect(t) => validate_collect(t, db, new_block).await?,
            _ => new_balance,
        };
    }

    if new_balance == new_data.balance as i128 && new_balance >= 0 {
        return Ok(());
    }

    Err(Validation::TxValidationError.into())
}

// Verifies the block height and previous block
fn verify_previous_block(new_block: &SignedBlock, prev_block: &SignedBlock) -> Result<()> {
    debug!("verify previous block");
    let new_data = new_block.data.as_ref().ok_or(Node::BlockDataNotFound)?;
    let prev_data = prev_block.data.as_ref().ok_or(Node::BlockDataNotFound)?;

    if new_data.height - 1 != prev_data.height {
        return Err(Validation::BlockHeightError.into());
    }
    if new_data.previous != prev_block.get_id()?.to_vec() {
        return Err(Validation::PreviousBlockError.into());
    }
    Ok(())
}

// Verifies the block height and previous block
fn verify_account_genesis_block() -> Result<()> {
    unimplemented!()
}

#[allow(clippy::borrowed_box)]
async fn validate_collect(tx: &TxClaim, db: &Box<dyn Database>, block: &SignedBlock) -> Result<i128> {
    // check DB for send with id tx_id
    // TODO: check already collected
    debug!("verify claim transactions");
    let tx_id = match tx.transaction_id.clone().try_into() {
        Ok(a) => a,
        Err(_) => return Err(Node::TxNotFound.into()),
    };
    let db_response = db.get_transaction_by_id(tx_id).await;
    let transaction = match db_response {
        Ok(t) => t,
        Err(_) => return Err(Validation::TxValidationError.into()),
    };

    let sendtrx = match &transaction.data {
        Some(Data::TxSend(t)) => t,
        Some(_) => return Err(Validation::MissmatchedTx.into()),
        None => return Err(Validation::SendTxNotFound.into()),
    };

    let account_id = generate_account_address(block.public_key.to_vec())?;
    if account_id.to_vec() != sendtrx.receiver {
        return Err(Validation::TxValidationError.into());
    }

    Ok(sendtrx.amount.into())
}

#[cfg(test)]
mod tests {
    use crate::validation::block::{verify_previous_block, verify_transactions};
    use crate::ChampState;
    use anyhow::Result;
    // use pog_proto::api::transaction::TxClaim;
    use pog_proto::api::{
        signed_block::BlockData,
        transaction::{Data, TxSend},
        SignedBlock, Transaction,
    };

    #[test]
    fn test_verify_previous_block() -> Result<()> {
        let prev_block = SignedBlock {
            signature: b"thisIsNewSignature".to_vec(),
            public_key: b"someOtherKey".to_vec(),
            timestamp: 1,
            data: Some(BlockData {
                version: 0,
                signature_type: 0,
                balance: 100,
                height: 4,
                previous: b"blockBeforeMe".to_vec(),
                transactions: [].to_vec(),
            }),
        };
        let new_block = SignedBlock {
            signature: b"signedByMe".to_vec(),
            public_key: b"someKey".to_vec(),
            timestamp: 1,
            data: Some(BlockData {
                version: 0,
                signature_type: 0,
                balance: 100,
                height: 5,
                previous: prev_block.get_id().expect("get Block ID").to_vec(),
                transactions: [].to_vec(),
            }),
        };

        assert!(verify_previous_block(&new_block, &prev_block).is_ok());
        Ok(())
    }

    #[tokio::test]
    async fn test_verify_transactions() -> Result<()> {
        let prev_block = SignedBlock {
            signature: b"thisIsNewSignature".to_vec(),
            public_key: b"someOtherKey".to_vec(),
            timestamp: 1,
            data: Some(BlockData {
                version: 0,
                signature_type: 0,
                balance: 100,
                height: 4,
                previous: b"blockBeforeMe".to_vec(),
                transactions: vec![],
            }),
        };
        let block = SignedBlock {
            signature: b"signedByMe".to_vec(),
            public_key: b"someKey".to_vec(),
            timestamp: 1,
            data: Some(BlockData {
                version: 0,
                signature_type: 0,
                balance: 40,
                height: 5,
                previous: prev_block.get_id().expect("get Block ID").to_vec(),
                transactions: vec![
                    Transaction {
                        data: Some(Data::TxSend(TxSend {
                            receiver: b"somereceiver".to_vec(),
                            amount: 10,
                            data: [].to_vec(),
                        })),
                    },
                    Transaction {
                        data: Some(Data::TxSend(TxSend {
                            receiver: b"somereceiver".to_vec(),
                            amount: 50,
                            data: [].to_vec(),
                        })),
                    },
                ],
            }),
        };
        let check_claim_tx = Transaction {
            data: Some(Data::TxSend(TxSend {
                receiver: b"somereceiver".to_vec(),
                amount: 10,
                data: vec![],
            })),
        };
        let data_block_1 = SignedBlock {
            signature: b"data_block_one".to_vec(),
            public_key: b"key_one".to_vec(),
            timestamp: 1,
            data: Some(BlockData {
                version: 0,
                signature_type: 0,
                balance: 40,
                height: 0,
                previous: prev_block.get_id().expect("get Block ID").to_vec(),
                transactions: vec![check_claim_tx.clone()],
            }),
        };
        // TODO: Fix this
        // let check_claim_previous = Block {
        //     signature: b"data_block_one".to_vec(),
        //     public_key: b"key_one".to_vec(),
        //     timestamp: 1,
        //     data: Some(BlockData {
        //         version: 0,
        //         signature_type: 0,
        //         balance: 40,
        //         height: 5,
        //         previous: Some(b"some_previous".to_vec()),
        //         transactions: vec![],
        //     }),
        // };
        // let check_claim = Block {
        //     signature: b"data_block_one".to_vec(),
        //     public_key: b"key_one".to_vec(),
        //     timestamp: 1,
        //     data: Some(BlockData {
        //         version: 0,
        //         signature_type: 0,
        //         balance: 50,
        //         height: 6,
        //         previous: Some(check_claim_previous.get_id().expect("get block ID").to_vec()),
        //         transactions: vec![Transaction {
        //             data: Some(Data::TxCollect(TxClaim {
        //                 transaction_id: check_claim_tx
        //                     .get_id(data_block_1.get_id().expect("get block ID"))
        //                     .expect("get Tx ID")
        //                     .to_vec(),
        //             })),
        //         }],
        //     }),
        // };

        let state = ChampState::mock().await;
        state.db.lock().await.add_block(data_block_1).await.expect("block should be added");
        assert!(verify_transactions(&block, &prev_block, &state).await.is_ok());
        // assert_eq!(
        //    verify_transactions(&check_claim, &check_claim_previous, &state)
        //        .await
        //        .expect("tx should be verified. Tx Nr: 2"),
        //    ()
        // );

        Ok(())
    }
}
