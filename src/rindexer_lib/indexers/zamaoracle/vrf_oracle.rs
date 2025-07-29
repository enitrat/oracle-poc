#![allow(non_snake_case)]
use super::super::super::typings::zamaoracle::events::vrf_oracle::{
    no_extensions, RandomnessFulfilledEvent, RandomnessRequestedEvent, VRFOracleEventType,
};
use alloy::primitives::{I256, U256};
use rindexer::{
    event::callback_registry::EventCallbackRegistry, rindexer_error, rindexer_info,
    EthereumSqlTypeWrapper, PgType, RindexerColorize,
};
use std::path::PathBuf;
use std::sync::Arc;

async fn randomness_fulfilled_handler(
    manifest_path: &PathBuf,
    registry: &mut EventCallbackRegistry,
) {
    let handler = RandomnessFulfilledEvent::handler(|results, context| async move {
                                if results.is_empty() {
                                    return Ok(());
                                }



                    let mut postgres_bulk_data: Vec<Vec<EthereumSqlTypeWrapper>> = vec![];
                    let mut csv_bulk_data: Vec<Vec<String>> = vec![];
                    for result in results.iter() {
                        csv_bulk_data.push(vec![result.tx_information.address.to_string(),result.event_data.requestId.iter().map(|byte| format!("{byte:02x}")).collect::<Vec<_>>().join(""),
result.event_data.randomness.to_string(),
result.tx_information.transaction_hash.to_string(),result.tx_information.block_number.to_string(),result.tx_information.block_hash.to_string(),result.tx_information.network.to_string(),result.tx_information.transaction_index.to_string(),result.tx_information.log_index.to_string()]);
                        let data = vec![
EthereumSqlTypeWrapper::Address(result.tx_information.address),
EthereumSqlTypeWrapper::Bytes(result.event_data.requestId.into()),
EthereumSqlTypeWrapper::U256(U256::from(result.event_data.randomness)),
EthereumSqlTypeWrapper::B256(result.tx_information.transaction_hash),
EthereumSqlTypeWrapper::U64(result.tx_information.block_number),
EthereumSqlTypeWrapper::B256(result.tx_information.block_hash),
EthereumSqlTypeWrapper::String(result.tx_information.network.to_string()),
EthereumSqlTypeWrapper::U64(result.tx_information.transaction_index),
EthereumSqlTypeWrapper::U256(result.tx_information.log_index)
];
                        postgres_bulk_data.push(data);
                    }

                    if !csv_bulk_data.is_empty() {
                        let csv_result = context.csv.append_bulk(csv_bulk_data).await;
                        if let Err(e) = csv_result {
                            rindexer_error!("VRFOracleEventType::RandomnessFulfilled inserting csv data: {:?}", e);
                            return Err(e.to_string());
                        }
                    }

                    if postgres_bulk_data.is_empty() {
                        return Ok(());
                    }

                    let rows = ["contract_address".to_string(), "request_id".to_string(), "randomness".to_string(), "tx_hash".to_string(), "block_number".to_string(), "block_hash".to_string(), "network".to_string(), "tx_index".to_string(), "log_index".to_string()];

                    if postgres_bulk_data.len() > 100 {
                        let result = context
                            .database
                            .bulk_insert_via_copy(
                                "zamaoracle_vrf_oracle.randomness_fulfilled",
                                &rows,
                                &postgres_bulk_data
                                    .first()
                                    .ok_or("No first element in bulk data, impossible")?
                                    .iter()
                                    .map(|param| param.to_type())
                                    .collect::<Vec<PgType>>(),
                                &postgres_bulk_data,
                            )
                            .await;

                        if let Err(e) = result {
                            rindexer_error!("VRFOracleEventType::RandomnessFulfilled inserting bulk data via COPY: {:?}", e);
                            return Err(e.to_string());
                        }
                        } else {
                            let result = context
                                .database
                                .bulk_insert(
                                    "zamaoracle_vrf_oracle.randomness_fulfilled",
                                    &rows,
                                    &postgres_bulk_data,
                                )
                                .await;

                            if let Err(e) = result {
                                rindexer_error!("VRFOracleEventType::RandomnessFulfilled inserting bulk data via INSERT: {:?}", e);
                                return Err(e.to_string());
                            }
                    }


                                rindexer_info!(
                                    "VRFOracle::RandomnessFulfilled - {} - {} events",
                                    "INDEXED".green(),
                                    results.len(),
                                );

                                Ok(())
                            },
                            no_extensions(),
                          )
                          .await;

    VRFOracleEventType::RandomnessFulfilled(handler)
        .register(manifest_path, registry)
        .await;
}

async fn randomness_requested_handler(
    manifest_path: &PathBuf,
    registry: &mut EventCallbackRegistry,
) {
    let handler = RandomnessRequestedEvent::handler(|results, context| async move {
                                if results.is_empty() {
                                    return Ok(());
                                }



                    let mut postgres_bulk_data: Vec<Vec<EthereumSqlTypeWrapper>> = vec![];
                    let mut csv_bulk_data: Vec<Vec<String>> = vec![];
                    for result in results.iter() {
                        csv_bulk_data.push(vec![result.tx_information.address.to_string(),result.event_data.requestId.iter().map(|byte| format!("{byte:02x}")).collect::<Vec<_>>().join(""),
result.event_data.requester.to_string(),
result.event_data.paid.to_string(),
result.tx_information.transaction_hash.to_string(),result.tx_information.block_number.to_string(),result.tx_information.block_hash.to_string(),result.tx_information.network.to_string(),result.tx_information.transaction_index.to_string(),result.tx_information.log_index.to_string()]);
                        let data = vec![
EthereumSqlTypeWrapper::Address(result.tx_information.address),
EthereumSqlTypeWrapper::Bytes(result.event_data.requestId.into()),
EthereumSqlTypeWrapper::Address(result.event_data.requester),
EthereumSqlTypeWrapper::U256(U256::from(result.event_data.paid)),
EthereumSqlTypeWrapper::B256(result.tx_information.transaction_hash),
EthereumSqlTypeWrapper::U64(result.tx_information.block_number),
EthereumSqlTypeWrapper::B256(result.tx_information.block_hash),
EthereumSqlTypeWrapper::String(result.tx_information.network.to_string()),
EthereumSqlTypeWrapper::U64(result.tx_information.transaction_index),
EthereumSqlTypeWrapper::U256(result.tx_information.log_index)
];
                        postgres_bulk_data.push(data);
                    }

                    if !csv_bulk_data.is_empty() {
                        let csv_result = context.csv.append_bulk(csv_bulk_data).await;
                        if let Err(e) = csv_result {
                            rindexer_error!("VRFOracleEventType::RandomnessRequested inserting csv data: {:?}", e);
                            return Err(e.to_string());
                        }
                    }

                    if postgres_bulk_data.is_empty() {
                        return Ok(());
                    }

                    let rows = ["contract_address".to_string(), "request_id".to_string(), "requester".to_string(), "paid".to_string(), "tx_hash".to_string(), "block_number".to_string(), "block_hash".to_string(), "network".to_string(), "tx_index".to_string(), "log_index".to_string()];

                    if postgres_bulk_data.len() > 100 {
                        let result = context
                            .database
                            .bulk_insert_via_copy(
                                "zamaoracle_vrf_oracle.randomness_requested",
                                &rows,
                                &postgres_bulk_data
                                    .first()
                                    .ok_or("No first element in bulk data, impossible")?
                                    .iter()
                                    .map(|param| param.to_type())
                                    .collect::<Vec<PgType>>(),
                                &postgres_bulk_data,
                            )
                            .await;

                        if let Err(e) = result {
                            rindexer_error!("VRFOracleEventType::RandomnessRequested inserting bulk data via COPY: {:?}", e);
                            return Err(e.to_string());
                        }
                        } else {
                            let result = context
                                .database
                                .bulk_insert(
                                    "zamaoracle_vrf_oracle.randomness_requested",
                                    &rows,
                                    &postgres_bulk_data,
                                )
                                .await;

                            if let Err(e) = result {
                                rindexer_error!("VRFOracleEventType::RandomnessRequested inserting bulk data via INSERT: {:?}", e);
                                return Err(e.to_string());
                            }
                    }


                                rindexer_info!(
                                    "VRFOracle::RandomnessRequested - {} - {} events",
                                    "INDEXED".green(),
                                    results.len(),
                                );

                                Ok(())
                            },
                            no_extensions(),
                          )
                          .await;

    VRFOracleEventType::RandomnessRequested(handler)
        .register(manifest_path, registry)
        .await;
}
pub async fn vrf_oracle_handlers(manifest_path: &PathBuf, registry: &mut EventCallbackRegistry) {
    randomness_fulfilled_handler(manifest_path, registry).await;

    randomness_requested_handler(manifest_path, registry).await;
}
