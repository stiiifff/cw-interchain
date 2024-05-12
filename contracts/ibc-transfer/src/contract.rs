use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, StdResult};
use neutron_sdk::{
    bindings::{msg::NeutronMsg, query::NeutronQuery},
    NeutronResult,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg};

// Example Sudo payload type
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct SudoPayload {
    pub data: String,
}

pub fn instantiate(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: InstantiateMsg,
) -> StdResult<Response> {
    Ok(Response::default())
}

pub fn execute(
    deps: DepsMut<NeutronQuery>,
    env: Env,
    _: MessageInfo,
    msg: ExecuteMsg,
) -> NeutronResult<Response<NeutronMsg>> {
    match msg {
        ExecuteMsg::IbcSend {
            channel,
            to,
            denom,
            amount,
            memo,
        } => {
            let sudo_payload = SudoPayload {
                data: "foobar".to_string(),
            };
            cw_ibc_transfer::ibc_send(
                deps,
                env,
                channel,
                to,
                denom,
                amount,
                memo,
                &sudo_payload,
                None,
            )
        }
    }
}

pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    deps.api.debug("WASMDEBUG: migrate");
    Ok(Response::default())
}
