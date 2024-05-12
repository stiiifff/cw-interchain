use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg};
use contract::SudoPayload;
use cosmwasm_std::{
    entry_point, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdError, StdResult,
};
use cw2::{ensure_from_older_version, set_contract_version};
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    sudo::msg::SudoMsg,
    NeutronResult,
};

pub mod contract;
pub mod msg;

const CONTRACT_NAME: &str = concat!("crates.io:", env!("CARGO_PKG_NAME"));
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, StdError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    contract::instantiate(deps, env, info, msg)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut<NeutronQuery>,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> NeutronResult<Response<NeutronMsg>> {
    contract::execute(deps, env, info, msg)
}

// #[cfg_attr(not(feature = "library"), entry_point)]
// pub fn query(deps: Deps, env: Env, msg: msg::QueryMsg) -> StdResult<Binary> {
//     contract::query(deps, env, msg)
// }

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, env: Env, msg: MigrateMsg) -> StdResult<Response> {
    let _original_version =
        ensure_from_older_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    contract::migrate(deps, env, msg)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> StdResult<Response> {
    if cw_ibc_transfer::is_ibc_transfer_reply(&msg) {
        return cw_ibc_transfer::handle_ibc_transfer_reply::<SudoPayload>(deps, env, msg);
    }
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, env: Env, msg: SudoMsg) -> StdResult<Response> {
    if cw_ibc_transfer::is_ibc_transfer_sudo(&msg) {
        return cw_ibc_transfer::handle_ibc_transfer_sudo(
            deps,
            env,
            msg,
            ibc_transfer_sudo_callback,
        );
    }
    Ok(Response::default())
}

// Callback handler for Sudo payload
// Different logic is possible depending on the type of the payload we saved in msg_with_sudo_callback() call
// This allows us to distinguish different transfer message from each other.
// For example some protocols can send one transfer to refund user for some action and another transfer to top up some balance.
// Such different actions may require different handling of their responses.
fn ibc_transfer_sudo_callback(deps: Deps, payload: SudoPayload) -> StdResult<Response> {
    deps.api.debug(
        format!(
            "WASMDEBUG: ibc_transfer_sudo_callback: sudo payload: {:?}",
            payload
        )
        .as_str(),
    );
    Ok(Response::new())
}
