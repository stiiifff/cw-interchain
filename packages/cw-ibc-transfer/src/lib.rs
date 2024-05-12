use cosmwasm_std::{
    coin, from_json, Binary, CosmosMsg, Deps, DepsMut, Env, Reply, Response, StdError, StdResult,
    SubMsg,
};
use neutron_sdk::{
    bindings::{
        msg::{IbcFee, MsgIbcTransferResponse, NeutronMsg},
        query::NeutronQuery,
    },
    query::min_ibc_fee::query_min_ibc_fee,
    sudo::msg::{RequestPacket, RequestPacketTimeoutHeight, SudoMsg},
    NeutronResult,
};
use serde::{de::DeserializeOwned, Serialize};
use state::SUDO_PAYLOAD;

use crate::state::{
    read_reply_payload, read_sudo_payload, save_reply_payload, save_sudo_payload,
    IBC_SUDO_ID_RANGE_END, IBC_SUDO_ID_RANGE_START,
};

mod state;

// Default timeout for IbcTransfer is 10000000 blocks
const DEFAULT_TIMEOUT_HEIGHT: u64 = 10000000;
const FEE_DENOM: &str = "untrn";

#[allow(clippy::too_many_arguments)]
pub fn ibc_send<TSudoPayload>(
    mut deps: DepsMut<NeutronQuery>,
    env: Env,
    channel: String,
    to: String,
    denom: String,
    amount: u128,
    memo: String,
    sudo_payload: &TSudoPayload,
    timeout_height: Option<u64>,
) -> NeutronResult<Response<NeutronMsg>>
where
    TSudoPayload: Serialize + ?Sized,
{
    // contract must pay for relaying of acknowledgements
    // See more info here: https://docs.neutron.org/neutron/feerefunder/overview
    let fee = min_ntrn_ibc_fee(query_min_ibc_fee(deps.as_ref())?.min_fee);
    let coin = coin(amount, denom.clone());

    let msg = NeutronMsg::IbcTransfer {
        source_port: "transfer".to_string(),
        source_channel: channel.clone(),
        sender: env.contract.address.to_string(),
        receiver: to.clone(),
        token: coin,
        timeout_height: RequestPacketTimeoutHeight {
            revision_number: Some(2),
            revision_height: timeout_height.or(Some(DEFAULT_TIMEOUT_HEIGHT)),
        },
        timeout_timestamp: timeout_height.unwrap_or(env.block.height + DEFAULT_TIMEOUT_HEIGHT),
        memo,
        fee,
    };

    let submsg = msg_with_sudo_callback(deps.branch(), msg, sudo_payload)?;

    deps.as_ref()
        .api
        .debug(format!("WASMDEBUG: ibc_send: sent submsg: {:?}", submsg).as_str());

    Ok(Response::default().add_submessages(vec![submsg]))
}

pub fn handle_ibc_transfer_reply<TSudoPayload>(
    deps: DepsMut,
    env: Env,
    msg: Reply,
) -> StdResult<Response>
where
    TSudoPayload: Serialize + ?Sized + DeserializeOwned,
{
    if is_ibc_transfer_reply(&msg) {
        prepare_sudo_payload::<TSudoPayload>(deps, env, msg)
    } else {
        Err(StdError::generic_err(format!(
            "unsupported reply message id {}",
            msg.id
        )))
    }
}

// saves payload to process later to the storage and returns a SubmitTX Cosmos SubMsg with necessary reply id
fn msg_with_sudo_callback<TPayload, C: Into<CosmosMsg<TMsg>>, TMsg>(
    deps: DepsMut<NeutronQuery>,
    msg: C,
    payload: &TPayload,
) -> StdResult<SubMsg<TMsg>>
where
    TPayload: Serialize + ?Sized,
{
    let id = save_reply_payload(deps.storage, payload)?;
    Ok(SubMsg::reply_on_success(msg, id))
}

// prepare_sudo_payload is called from reply handler
// The method is used to extract sequence id and channel from SubmitTxResponse to process sudo payload defined in msg_with_sudo_callback later in Sudo handler.
// Such flow msg_with_sudo_callback() -> reply() -> prepare_sudo_payload() -> sudo() allows you "attach" some payload to your Transfer message
// and process this payload when an acknowledgement for the SubmitTx message is received in Sudo handler
fn prepare_sudo_payload<T>(mut deps: DepsMut, _env: Env, msg: Reply) -> StdResult<Response>
where
    T: Serialize + ?Sized + DeserializeOwned,
{
    let payload = read_reply_payload::<T>(deps.storage, msg.id)?;
    let resp: MsgIbcTransferResponse = from_json(
        msg.result
            .into_result()
            .map_err(StdError::generic_err)?
            .data
            .ok_or_else(|| StdError::generic_err("no result"))?,
    )
    .map_err(|e| StdError::generic_err(format!("failed to parse response: {:?}", e)))?;
    let seq_id = resp.sequence_id;
    let channel_id = resp.channel;
    save_sudo_payload(deps.branch().storage, channel_id, seq_id, &payload)?;
    Ok(Response::new())
}

pub fn is_ibc_transfer_reply(msg: &Reply) -> bool {
    matches!(msg.id, IBC_SUDO_ID_RANGE_START..=IBC_SUDO_ID_RANGE_END)
}

pub fn is_ibc_transfer_sudo(msg: &SudoMsg) -> bool {
    matches!(
        msg,
        SudoMsg::Response { .. } | SudoMsg::Error { .. } | SudoMsg::Timeout { .. }
    )
}

pub fn handle_ibc_transfer_sudo<TSudoPayload, FSudoPayloadHandler>(
    deps: DepsMut,
    _env: Env,
    msg: SudoMsg,
    sudo_payload_handler: FSudoPayloadHandler,
) -> StdResult<Response>
where
    TSudoPayload: DeserializeOwned,
    FSudoPayloadHandler: FnOnce(Deps, TSudoPayload) -> StdResult<Response>,
{
    if is_ibc_transfer_sudo(&msg) {
        match msg {
            // For handling successful (non-error) acknowledgements
            SudoMsg::Response { request, data } => {
                sudo_response(deps, request, data, sudo_payload_handler)
            }
            // For handling error acknowledgements
            SudoMsg::Error { request, details } => sudo_error(deps, request, details),
            // For handling error timeouts
            SudoMsg::Timeout { request } => sudo_timeout(deps, request),
            _ => unreachable!(),
        }
    } else {
        Err(StdError::generic_err(format!(
            "unsupported sudo message {:?}",
            msg
        )))
    }
}

fn sudo_error(deps: DepsMut, req: RequestPacket, data: String) -> StdResult<Response> {
    deps.api.debug(
        format!(
            "WASMDEBUG: sudo_error: sudo error received: {:?} {}",
            req, data
        )
        .as_str(),
    );
    Ok(Response::new())
}

fn sudo_timeout(deps: DepsMut, req: RequestPacket) -> StdResult<Response> {
    deps.api.debug(
        format!(
            "WASMDEBUG: sudo_timeout: sudo timeout ack received: {:?}",
            req
        )
        .as_str(),
    );
    Ok(Response::new())
}

fn sudo_response<TSudoPayload, FSudoPayloadHandler>(
    deps: DepsMut,
    req: RequestPacket,
    data: Binary,
    sudo_payload_handler: FSudoPayloadHandler,
) -> StdResult<Response>
where
    TSudoPayload: DeserializeOwned,
    FSudoPayloadHandler: FnOnce(Deps, TSudoPayload) -> StdResult<Response>,
{
    deps.api.debug(
        format!(
            "WASMDEBUG: sudo_response: sudo received: {:?} {}",
            req, data
        )
        .as_str(),
    );
    let seq_id = req
        .sequence
        .ok_or_else(|| StdError::generic_err("sequence not found"))?;
    let channel_id = req
        .source_channel
        .ok_or_else(|| StdError::generic_err("channel_id not found"))?;

    let payload = read_sudo_payload::<TSudoPayload>(deps.storage, channel_id.clone(), seq_id)?;

    // at this place we can safely remove the data under (channel_id, seq_id) key
    SUDO_PAYLOAD.remove(deps.storage, (channel_id, seq_id));

    sudo_payload_handler(deps.as_ref(), payload)
}

fn min_ntrn_ibc_fee(fee: IbcFee) -> IbcFee {
    IbcFee {
        recv_fee: fee.recv_fee,
        ack_fee: fee
            .ack_fee
            .into_iter()
            .filter(|a| a.denom == FEE_DENOM)
            .collect(),
        timeout_fee: fee
            .timeout_fee
            .into_iter()
            .filter(|a| a.denom == FEE_DENOM)
            .collect(),
    }
}
