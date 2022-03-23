use std::collections::btree_set::Difference;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, from_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Uint128,
    WasmMsg, WasmQuery, QueryRequest, CosmosMsg, Order, Addr, Decimal, Storage, Api
};
use cw2::{get_contract_version, set_contract_version};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg, Cw20QueryMsg, Cw20CoinVerified};
use cw20::{TokenInfoResponse, Balance};
use cw_utils::{maybe_addr};
use cw_storage_plus::Bound;
use crate::error::ContractError;
use crate::msg::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg, ReceiveMsg, StakerListResponse, StakerInfo, CountInfo, StakerResponse
};
use crate::state::{
    Config, CONFIG, STAKERS
};

// Version info, for migration info
const CONTRACT_NAME: &str = "gfotstaking";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

// const DAILY_FOT_AMOUNT:u128 = 100_000_000_000_000u128;
const MULTIPLE:u128 = 10_000_000_000u128;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let owner = msg
        .owner
        .map_or(Ok(info.sender), |o| deps.api.addr_validate(&o))?;

    let config = Config {
        owner: Some(owner),
        fot_token_address: msg.fot_token_address,
        bfot_token_address:msg.bfot_token_address,
        gfot_token_address: msg.gfot_token_address,
        fot_amount: Uint128::zero(),
        gfot_amount: Uint128::zero(),
        daily_fot_amount: msg.daily_fot_amount,
        apy_prefix: msg.apy_prefix,
        reward_interval: msg.reward_interval
    };
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateConfig { new_owner } => execute_update_config(deps, info, new_owner),
        ExecuteMsg::UpdateConstants { daily_fot_amount, apy_prefix , reward_interval} => execute_update_constants(deps, info, daily_fot_amount, apy_prefix, reward_interval),
        ExecuteMsg::Receive(msg) => try_receive(deps, env, info, msg),
        ExecuteMsg::WithdrawFot {} => try_withdraw_fot(deps, env, info),
        ExecuteMsg::WithdrawGFot {} => try_withdraw_gfot(deps, env, info),
        ExecuteMsg::ClaimReward {} => try_claim_reward(deps, env, info),
        ExecuteMsg::Unstake {} => try_unstake(deps, env, info),
        ExecuteMsg::AddStakers { stakers } => execute_add_stakers(deps, info, stakers),
        ExecuteMsg::RemoveStaker { address } => execute_remove_staker(deps, info, address),
        ExecuteMsg::RemoveAllStakers { start_after, limit } => execute_remove_all_stakers(deps, info, start_after, limit),
    }
}

pub fn update_reward (
    storage: &mut dyn Storage,
    api: &dyn Api,
    env: Env,
    address: Addr,
    start_after:Option<String>
) -> Result<Response, ContractError> {
    
    let exists = STAKERS.may_load(storage, address.clone())?;
    let (mut amount, mut reward, mut last_time) = (Uint128::zero(), Uint128::zero(), 0u64);
    if exists.is_some() {
        (amount, reward, last_time) = exists.unwrap();
    }

    if last_time == 0u64 {
        last_time = env.block.time.seconds();
    }
    STAKERS.save(storage, address.clone(), &(amount, reward, last_time))?;

    let cfg = CONFIG.load(storage)?;
    let delta = env.block.time.seconds() / cfg.reward_interval - last_time / cfg.reward_interval;
    
    if cfg.gfot_amount > Uint128::zero() && amount > Uint128::zero() && delta > 0 {
        reward += cfg.daily_fot_amount * Uint128::from(delta) * amount / cfg.gfot_amount;
        STAKERS.save(storage, address.clone(), &(amount, reward, env.block.time.seconds()))?;
    }

    Ok(Response::default())
}

pub fn try_receive(
    deps: DepsMut, 
    env: Env,
    info: MessageInfo, 
    wrapper: Cw20ReceiveMsg
) -> Result<Response, ContractError> {
    
    let mut cfg = CONFIG.load(deps.storage)?;
    
    if wrapper.amount == Uint128::zero() {
        return Err(ContractError::InvalidInput {});
    }
    let user_addr = &deps.api.addr_validate(&wrapper.sender)?;

    // Staking case
    if info.sender == cfg.gfot_token_address {
        update_reward(deps.storage, deps.api, env, user_addr.clone(), None)?;
        let (mut amount, mut reward, mut last_time) = STAKERS.load(deps.storage, user_addr.clone())?;
        amount += wrapper.amount;
        STAKERS.save(deps.storage, user_addr.clone(), &(amount, reward, last_time))?;
        
        cfg.gfot_amount = cfg.gfot_amount + wrapper.amount;
        CONFIG.save(deps.storage, &cfg)?;

        return Ok(Response::new()
            .add_attributes(vec![
                attr("action", "stake"),
                attr("address", user_addr),
                attr("amount", wrapper.amount)
            ]));

    } else if info.sender == cfg.fot_token_address {
        //Just receive in contract cache and update config
        cfg.fot_amount = cfg.fot_amount + wrapper.amount;
        CONFIG.save(deps.storage, &cfg)?;

        return Ok(Response::new()
            .add_attributes(vec![
                attr("action", "fund"),
                attr("address", user_addr),
                attr("amount", wrapper.amount),
            ]));

    } else {
        return Err(ContractError::UnacceptableToken {})
    }
}

pub fn try_claim_reward(
    deps: DepsMut,
    env: Env,
    info: MessageInfo
) -> Result<Response, ContractError> {

    update_reward(deps.storage, deps.api, env, info.sender.clone(), None)?;
    let mut cfg = CONFIG.load(deps.storage)?;

    let (amount, reward, last_time) = STAKERS.load(deps.storage, info.sender.clone())?;
    
    if reward == Uint128::zero() {
        return Err(ContractError::NoReward {});
    }
    if cfg.fot_amount < Uint128::from(reward) {
        return Err(ContractError::NotEnoughFOT {});
    }
    
    cfg.fot_amount -= Uint128::from(reward);
    CONFIG.save(deps.storage, &cfg)?;
    
    if amount == Uint128::zero() {
        STAKERS.remove(deps.storage, info.sender.clone());
    } else {
        STAKERS.save(deps.storage, info.sender.clone(), &(amount, Uint128::zero(), last_time))?;
    }

    let exec_cw20_transfer = WasmMsg::Execute {
        contract_addr: cfg.fot_token_address.clone().into(),
        msg: to_binary(&Cw20ExecuteMsg::Transfer {
            recipient: info.sender.clone().into(),
            amount: Uint128::from(reward),
        })?,
        funds: vec![],
    };

    
    return Ok(Response::new()
        .add_message(exec_cw20_transfer)
        .add_attributes(vec![
            attr("action", "claim_reward"),
            attr("address", info.sender.clone()),
            attr("fot_amount", Uint128::from(reward)),
        ]));
}

pub fn try_unstake(
    deps: DepsMut,
    env: Env,
    info: MessageInfo
) -> Result<Response, ContractError> {

    update_reward(deps.storage, deps.api, env, info.sender.clone(), None)?;
    let mut cfg = CONFIG.load(deps.storage)?;
    let (amount, reward, last_time) = STAKERS.load(deps.storage, info.sender.clone())?;
    
    if amount == Uint128::zero() {
        return Err(ContractError::NoStaked {});
    }
    if cfg.gfot_amount < Uint128::from(amount) {
        return Err(ContractError::NotEnoughgFOT {});
    }

    cfg.gfot_amount -= Uint128::from(amount);

    CONFIG.save(deps.storage, &cfg)?;

    if reward == Uint128::zero() {
        STAKERS.remove(deps.storage, info.sender.clone());
    } else {
        STAKERS.save(deps.storage, info.sender.clone(), &(Uint128::zero(), reward, last_time))?;
    }

    let exec_cw20_transfer = WasmMsg::Execute {
        contract_addr: cfg.gfot_token_address.clone().into(),
        msg: to_binary(&Cw20ExecuteMsg::Transfer {
            recipient: info.sender.clone().into(),
            amount: Uint128::from(amount),
        })?,
        funds: vec![],
    };
    
    
    return Ok(Response::new()
        .add_message(exec_cw20_transfer)
        .add_attributes(vec![
            attr("action", "unstake"),
            attr("address", info.sender.clone()),
            attr("gfot_amount", Uint128::from(amount)),
        ]));
}

pub fn check_owner(
    deps: &DepsMut,
    info: &MessageInfo
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;
    let owner = cfg.owner.ok_or(ContractError::Unauthorized {})?;
    if info.sender != owner {
        return Err(ContractError::Unauthorized {})
    }
    Ok(Response::new().add_attribute("action", "check_owner"))
}

pub fn execute_update_config(
    deps: DepsMut,
    info: MessageInfo,
    new_owner: Option<String>,
) -> Result<Response, ContractError> {
    // authorize owner
    check_owner(&deps, &info)?;
    
    //test code for checking if check_owner works well
    // return Err(ContractError::InvalidInput {});
    // if owner some validated to addr, otherwise set to none
    let mut tmp_owner = None;
    if let Some(addr) = new_owner {
        tmp_owner = Some(deps.api.addr_validate(&addr)?)
    }

    CONFIG.update(deps.storage, |mut exists| -> StdResult<_> {
        exists.owner = tmp_owner;
        Ok(exists)
    })?;

    Ok(Response::new().add_attribute("action", "update_config"))
}

pub fn execute_update_constants(
    deps: DepsMut,
    info: MessageInfo,
    daily_fot_amount: Uint128,
    apy_prefix: Uint128,
    reward_interval: u64
) -> Result<Response, ContractError> {
    // authorize owner
    check_owner(&deps, &info)?;
    
    //test code for checking if check_owner works well
    // return Err(ContractError::InvalidInput {});
    // if owner some validated to addr, otherwise set to none
    
    CONFIG.update(deps.storage, |mut exists| -> StdResult<_> {
        exists.daily_fot_amount = daily_fot_amount;
        exists.apy_prefix = apy_prefix;
        exists.reward_interval = reward_interval;
        Ok(exists)
    })?;

    Ok(Response::new().add_attribute("action", "update_constants"))
}


pub fn execute_add_stakers(
    deps: DepsMut,
    info: MessageInfo,
    stakers: Vec<StakerInfo>
) -> Result<Response, ContractError> {
    // authorize owner
    check_owner(&deps, &info)?;

    for staker in stakers {
        STAKERS.save(deps.storage, staker.address.clone(), &(staker.amount, staker.reward, staker.last_time))?;
    }
    
    Ok(Response::new().add_attribute("action", "add_stakers"))
}


pub fn execute_remove_staker(
    deps: DepsMut,
    info: MessageInfo,
    address: Addr
) -> Result<Response, ContractError> {
    // authorize owner
    check_owner(&deps, &info)?;
    
    STAKERS.remove(deps.storage, address.clone());
    
    Ok(Response::new().add_attribute("action", "remove_staker"))
}



pub fn execute_remove_all_stakers(
    deps: DepsMut,
    info: MessageInfo,
    start_after: Option<String>,
    limit: Option<u32>
) -> Result<Response, ContractError> {
    // authorize owner
    check_owner(&deps, &info)?;
    
    let addr = maybe_addr(deps.api, start_after)?;
    let start = addr.map(|addr| Bound::exclusive(addr.as_ref()));
    let stakers:StdResult<Vec<_>> = STAKERS
        .range(deps.storage, start, None, Order::Ascending)
        .map(|item| map_staker(item))
        .collect();

    if stakers.is_err() {
        return Err(ContractError::Map2ListFailed {})
    }
    
    for item in stakers.unwrap() {
        STAKERS.remove(deps.storage, item.address.clone());
    }
    
    Ok(Response::new().add_attribute("action", "remove_all_stakers"))
}

pub fn try_withdraw_fot(deps: DepsMut, env:Env, info: MessageInfo) -> Result<Response, ContractError> {
    
    check_owner(&deps, &info)?;
    let mut cfg = CONFIG.load(deps.storage)?;
    
    let fot_amount = cfg.fot_amount;
    cfg.fot_amount = Uint128::zero();
    CONFIG.save(deps.storage, &cfg)?;

    // create transfer cw20 msg
    let exec_cw20_transfer = WasmMsg::Execute {
        contract_addr: cfg.fot_token_address.clone().into(),
        msg: to_binary(&Cw20ExecuteMsg::Transfer {
            recipient: info.sender.clone().into(),
            amount: fot_amount,
        })?,
        funds: vec![],
    };

    return Ok(Response::new()
        .add_message(exec_cw20_transfer)
        .add_attributes(vec![
            attr("action", "fot_withdraw_all"),
            attr("address", info.sender.clone()),
            attr("fot_amount", fot_amount),
        ]));
}

pub fn try_withdraw_gfot(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    
    check_owner(&deps, &info)?;

    let mut cfg = CONFIG.load(deps.storage)?;
    let gfot_amount = cfg.gfot_amount;
    cfg.gfot_amount = Uint128::zero();

    CONFIG.save(deps.storage, &cfg)?;

    // create transfer cw20 msg
    let exec_cw20_transfer = WasmMsg::Execute {
        contract_addr: cfg.gfot_token_address.clone().into(),
        msg: to_binary(&Cw20ExecuteMsg::Transfer {
            recipient: info.sender.clone().into(),
            amount: gfot_amount,
        })?,
        funds: vec![],
    };

    return Ok(Response::new()
        .add_message(exec_cw20_transfer)
        .add_attributes(vec![
            attr("action", "gfot_withdraw_all"),
            attr("address", info.sender.clone()),
            attr("gfot_amount", gfot_amount),
        ]));
}



#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} 
            => to_binary(&query_config(deps)?),
        QueryMsg::Staker {address} 
            => to_binary(&query_staker(deps, address)?),
        QueryMsg::ListStakers {start_after, limit} 
            => to_binary(&query_list_stakers(deps, start_after, limit)?),
        QueryMsg::Apy {} 
            => to_binary(&query_apy(deps)?),
    }
}

pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let cfg = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        owner: cfg.owner.map(|o| o.into()),
        fot_token_address: cfg.fot_token_address.into(),
        bfot_token_address: cfg.bfot_token_address.into(),
        gfot_token_address: cfg.gfot_token_address.into(),
        fot_amount: cfg.fot_amount,
        gfot_amount: cfg.gfot_amount,
        daily_fot_amount: cfg.daily_fot_amount,
        apy_prefix: cfg.apy_prefix,
        reward_interval: cfg.reward_interval
    })
}

// settings for pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

fn query_staker(deps: Deps, address: Addr) -> StdResult<StakerResponse> {
    
    let exists = STAKERS.may_load(deps.storage, address.clone())?;
    let (mut amount, mut reward, mut last_time) = (Uint128::zero(), Uint128::zero(), 0u64);
    if exists.is_some() {
        (amount, reward, last_time) = exists.unwrap();
    } 
    Ok(StakerResponse {
        address,
        amount,
        reward,
        last_time
    })
}

fn map_staker(
    item: StdResult<(Addr, (Uint128, Uint128, u64))>,
) -> StdResult<StakerInfo> {
    item.map(|(address, (amount, reward, last_time))| {
        StakerInfo {
            address,
            amount,
            reward,
            last_time
        }
    })
}

fn query_list_stakers(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<StakerListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let addr = maybe_addr(deps.api, start_after)?;
    let start = addr.map(|addr| Bound::exclusive(addr.as_ref()));

    let stakers:StdResult<Vec<_>> = STAKERS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| map_staker(item))
        .collect();

    Ok(StakerListResponse { stakers: stakers? })
}

pub fn query_apy(deps: Deps) -> StdResult<Uint128> {
    let cfg = CONFIG.load(deps.storage)?;
    let total_staked_gfot = cfg.gfot_amount;
    if total_staked_gfot == Uint128::zero() {
        return Ok(Uint128::zero());
    }
    // For integer handling, return apy * MULTIPLE(10^10)

    // gFot_minting_cost: This is calculated by 1 / (GFOT current supply / 10^10 + 10000)
    let gfot_token_info: TokenInfoResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: cfg.gfot_token_address.clone().into(),
            msg: to_binary(&Cw20QueryMsg::TokenInfo {})?,
        }))?;
    
    let gfot_current_supply = Uint128::from(gfot_token_info.total_supply);

    let gfot_rate = (gfot_current_supply.checked_div(Uint128::from(10_000_000_000u128)).unwrap())
    .checked_add(Uint128::from(10000u128)).unwrap();
    // let gfot_minting_cost = 1.0 / (gfot_rate.u128() as f64);
    
    
    // bFot_receiving_ratio: This is calculated by 109 - (FOT current supply - 1) / 10^16
    // let fot_token_info: TokenInfoResponse =
    //     deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
    //         contract_addr: cfg.fot_token_address.clone().into(),
    //         msg: to_binary(&Cw20QueryMsg::TokenInfo {})?,
    //     }))?;

    // let fot_current_supply = Uint128::from(fot_token_info.total_supply);
    // let fot_rate = (fot_current_supply - Uint128::from(1u128)).checked_div(Uint128::from(10_000_000_000_000_000u128)).unwrap();
    // let bfot_receiving_ratio = Uint128::from(109u128) - fot_rate;

    
    Ok(cfg.apy_prefix.checked_mul(Uint128::from(MULTIPLE)).unwrap().checked_mul(Uint128::from(MULTIPLE)).unwrap().checked_div(gfot_rate).unwrap().checked_div(total_staked_gfot).unwrap())

    // Ok(cfg.apy_prefix.checked_mul(Uint128::from(MULTIPLE)).unwrap().checked_mul(bfot_receiving_ratio).unwrap().checked_div(gfot_rate).unwrap().checked_div(total_staked_gfot).unwrap())
    // Ok(Uint128::from(365000000u128).checked_mul(bfot_receiving_ratio).unwrap().checked_div(gfot_rate).unwrap())
}


#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    let version = get_contract_version(deps.storage)?;
    if version.contract != CONTRACT_NAME {
        return Err(ContractError::CannotMigrate {
            previous_contract: version.contract,
        });
    }
    Ok(Response::default())
}

