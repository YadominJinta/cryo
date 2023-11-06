use crate::*;
use ethers::prelude::*;
use polars::prelude::*;

/// columns for transactions
#[cryo_to_df::to_df(Datatype::StorageDiffs)]
#[derive(Default)]
pub struct StorageDiffs {
    pub(crate) n_rows: u64,
    pub(crate) block_number: Vec<Option<u32>>,
    pub(crate) transaction_index: Vec<Option<u32>>,
    pub(crate) transaction_hash: Vec<Option<Vec<u8>>>,
    pub(crate) address: Vec<Vec<u8>>,
    pub(crate) slot: Vec<Vec<u8>>,
    pub(crate) from_value: Vec<Vec<u8>>,
    pub(crate) to_value: Vec<Vec<u8>>,
    pub(crate) chain_id: Vec<u64>,
}

#[async_trait::async_trait]
impl Dataset for StorageDiffs {
    fn aliases() -> Vec<&'static str> {
        vec!["slot_diffs"]
    }
}

type BlockTxsTraces = (Option<u32>, Vec<Option<Vec<u8>>>, Vec<ethers::types::BlockTrace>);

#[async_trait::async_trait]
impl CollectByBlock for StorageDiffs {
    type Response = BlockTxsTraces;

    async fn extract(request: Params, source: Arc<Source>, query: Arc<Query>) -> R<Self::Response> {
        let schema = query.schemas.get_schema(&Datatype::StorageDiffs)?;
        let include_txs = schema.has_column("transaction_hash");
        source.fetcher.trace_block_state_diffs(request.block_number()? as u32, include_txs).await
    }

    fn transform(response: Self::Response, columns: &mut Self, query: &Arc<Query>) -> R<()> {
        process_storage_diffs(&response, columns, &query.schemas)
    }
}

#[async_trait::async_trait]
impl CollectByTransaction for StorageDiffs {
    type Response = BlockTxsTraces;

    async fn extract(request: Params, source: Arc<Source>, _: Arc<Query>) -> R<Self::Response> {
        source.fetcher.trace_transaction_state_diffs(request.transaction_hash()?).await
    }

    fn transform(response: Self::Response, columns: &mut Self, query: &Arc<Query>) -> R<()> {
        process_storage_diffs(&response, columns, &query.schemas)
    }
}

pub(crate) fn process_storage_diffs(
    response: &BlockTxsTraces,
    columns: &mut StorageDiffs,
    schemas: &Schemas,
) -> R<()> {
    let schema = schemas.get(&Datatype::StorageDiffs).ok_or(err("schema not provided"))?;
    let (block_number, txs, traces) = response;
    for (index, (trace, tx)) in traces.iter().zip(txs).enumerate() {
        if let Some(ethers::types::StateDiff(state_diffs)) = &trace.state_diff {
            for (addr, diff) in state_diffs.iter() {
                process_storage_diff(addr, &diff.storage, block_number, tx, index, columns, schema);
            }
        }
    }
    Ok(())
}

pub(crate) fn process_storage_diff(
    addr: &H160,
    diff: &std::collections::BTreeMap<H256, Diff<H256>>,
    block_number: &Option<u32>,
    transaction_hash: &Option<Vec<u8>>,
    transaction_index: usize,
    columns: &mut StorageDiffs,
    schema: &Table,
) {
    for (s, sub_diff) in diff.iter() {
        let (from, to) = match sub_diff {
            Diff::Same => continue,
            Diff::Born(value) => (H256::zero(), *value),
            Diff::Died(value) => (*value, H256::zero()),
            Diff::Changed(ChangedType { from, to }) => (*from, *to),
        };
        columns.n_rows += 1;
        store!(schema, columns, block_number, *block_number);
        store!(schema, columns, transaction_index, Some(transaction_index as u32));
        store!(schema, columns, transaction_hash, transaction_hash.clone());
        store!(schema, columns, slot, s.as_bytes().to_vec());
        store!(schema, columns, address, addr.as_bytes().to_vec());
        store!(schema, columns, from_value, from.as_bytes().to_vec());
        store!(schema, columns, to_value, to.as_bytes().to_vec());
    }
}
