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
const CONTRACT_NAME: &str = "marbleincentive";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const DAILY_REWARD_AMOUNT:u128 = 100_000_000_000u128;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let owner = msg
        .owner
        .map_or(Ok(info.sender), |o| deps.api.addr_validate(&o))?;

    let config = Config {
        owner: Some(owner),
        reward_token_address: msg.reward_token_address,
        lp_token_address: msg.lp_token_address,
        reward_amount: Uint128::zero(),
        lp_amount: Uint128::zero(),
        last_time: 0u64,
        addresses: vec![]
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
        ExecuteMsg::Receive(msg) => try_receive(deps, env, info, msg),
        ExecuteMsg::WithdrawReward {} => try_withdraw_reward(deps, env, info),
        ExecuteMsg::WithdrawLp {} => try_withdraw_lp(deps, env, info),
        ExecuteMsg::ClaimReward {} => try_claim_reward(deps, env, info),
        ExecuteMsg::Unstake {} => try_unstake(deps, env, info),
    }
}

pub fn update_total_reward (
    storage: &mut dyn Storage,
    api: &dyn Api,
    env: Env,
    start_after:Option<String>
) -> Result<Response, ContractError> {

    let mut cfg = CONFIG.load(storage)?;
    if cfg.last_time == 0u64 {
        cfg.last_time = env.block.time.seconds();
        CONFIG.save(storage, &cfg)?;
    }
    let before_time = cfg.last_time;
    
    cfg.last_time = env.block.time.seconds();
    
    let delta = cfg.last_time / 86400u64 - before_time / 86400u64;
    if delta > 0 {
        CONFIG.save(storage, &cfg)?;    
        let tot_reward_amount = Uint128::from(DAILY_REWARD_AMOUNT).checked_mul(Uint128::from(delta)).unwrap();

        let addr = maybe_addr(api, start_after)?;
        let start = addr.map(|addr| Bound::exclusive(addr.as_ref()));

        let stakers:StdResult<Vec<_>> = STAKERS
            .range(storage, start, None, Order::Ascending)
            .map(|item| map_staker(item))
            .collect();

        
        if stakers.is_err() {
            return Err(ContractError::Map2ListFailed {})
        }
        
        let mut tot_amount = Uint128::zero();
        let staker2 = stakers.unwrap().clone();
        let staker3 = staker2.clone();
        for item in staker2 {
            tot_amount += item.amount;
        }

        for item in staker3 {
            let mut new_reward = tot_reward_amount.checked_mul(item.amount).unwrap().checked_div(tot_amount).unwrap();
            new_reward = item.reward + new_reward;
            STAKERS.save(storage, item.address.clone(), &(item.amount, new_reward))?;
        }
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
    // let _msg: ReceiveMsg = from_binary(&wrapper.msg)?;
    // let balance = Cw20CoinVerified {
    //     address: info.sender.clone(),
    //     amount: wrapper.amount,
    // };
    
    let user_addr = &deps.api.addr_validate(&wrapper.sender)?;

    // Staking case
    if info.sender == cfg.lp_token_address {
        update_total_reward(deps.storage, deps.api, env, None)?;
        cfg = CONFIG.load(deps.storage)?;
        let exists = STAKERS.may_load(deps.storage, user_addr.clone())?;
        let (mut amount, mut reward) = (Uint128::zero(), Uint128::zero());
        if exists.is_some() {
            (amount, reward) = exists.unwrap();
        } else {
            cfg.addresses.push(user_addr.clone());
        }
        
        amount += wrapper.amount;
        STAKERS.save(deps.storage, user_addr.clone(), &(amount, reward))?;
        
        cfg.lp_amount = cfg.lp_amount + wrapper.amount;
        CONFIG.save(deps.storage, &cfg)?;

        

        return Ok(Response::new()
            .add_attributes(vec![
                attr("action", "stake"),
                attr("address", user_addr),
                attr("amount", wrapper.amount)
            ]));

    } else if info.sender == cfg.reward_token_address {
        //Just receive in contract cache and update config
        cfg.reward_amount = cfg.reward_amount + wrapper.amount;
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

    update_total_reward(deps.storage, deps.api, env, None)?;
    let mut cfg = CONFIG.load(deps.storage)?;

    let (amount, reward) = STAKERS.load(deps.storage, info.sender.clone())?;
    
    if reward == Uint128::zero() {
        return Err(ContractError::NoReward {});
    }
    if cfg.reward_amount < Uint128::from(reward) {
        return Err(ContractError::NotEnoughReward {});
    }
    
    cfg.reward_amount -= Uint128::from(reward);
    CONFIG.save(deps.storage, &cfg)?;
    STAKERS.save(deps.storage, info.sender.clone(), &(amount, Uint128::zero()))?;

    let exec_cw20_transfer = WasmMsg::Execute {
        contract_addr: cfg.reward_token_address.clone().into(),
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
            attr("reward_amount", Uint128::from(reward)),
        ]));
}

pub fn try_unstake(
    deps: DepsMut,
    env: Env,
    info: MessageInfo
) -> Result<Response, ContractError> {

    update_total_reward(deps.storage, deps.api, env, None)?;
    let mut cfg = CONFIG.load(deps.storage)?;
    let (amount, reward) = STAKERS.load(deps.storage, info.sender.clone())?;
    
    if amount == Uint128::zero() {
        return Err(ContractError::NoStaked {});
    }
    if cfg.lp_amount < Uint128::from(amount) {
        return Err(ContractError::NotEnoughLp {});
    }

    cfg.lp_amount -= Uint128::from(amount);

    CONFIG.save(deps.storage, &cfg)?;
    STAKERS.save(deps.storage, info.sender.clone(), &(Uint128::zero(), reward))?;

    let exec_cw20_transfer = WasmMsg::Execute {
        contract_addr: cfg.lp_token_address.clone().into(),
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
            attr("lp_amount", Uint128::from(amount)),
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

pub fn try_withdraw_reward(deps: DepsMut, env:Env, info: MessageInfo) -> Result<Response, ContractError> {

    
    check_owner(&deps, &info)?;
    update_total_reward(deps.storage, deps.api, env, None)?;
    let mut cfg = CONFIG.load(deps.storage)?;
    
    let reward_amount = cfg.reward_amount;
    cfg.reward_amount = Uint128::zero();
    CONFIG.save(deps.storage, &cfg)?;

    // create transfer cw20 msg
    let exec_cw20_transfer = WasmMsg::Execute {
        contract_addr: cfg.reward_token_address.clone().into(),
        msg: to_binary(&Cw20ExecuteMsg::Transfer {
            recipient: info.sender.clone().into(),
            amount: reward_amount,
        })?,
        funds: vec![],
    };

    return Ok(Response::new()
        .add_message(exec_cw20_transfer)
        .add_attributes(vec![
            attr("action", "fot_withdraw_all"),
            attr("address", info.sender.clone()),
            attr("reward_amount", reward_amount),
        ]));
}

pub fn try_withdraw_lp(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {

    
    check_owner(&deps, &info)?;
    update_total_reward(deps.storage, deps.api, env, None)?;
    let mut cfg = CONFIG.load(deps.storage)?;
    
    let lp_amount = cfg.lp_amount;
    cfg.lp_amount = Uint128::zero();
    CONFIG.save(deps.storage, &cfg)?;

    // create transfer cw20 msg
    let exec_cw20_transfer = WasmMsg::Execute {
        contract_addr: cfg.lp_token_address.clone().into(),
        msg: to_binary(&Cw20ExecuteMsg::Transfer {
            recipient: info.sender.clone().into(),
            amount: lp_amount,
        })?,
        funds: vec![],
    };

    

    return Ok(Response::new()
        .add_message(exec_cw20_transfer)
        .add_attributes(vec![
            attr("action", "lp_withdraw_all"),
            attr("address", info.sender.clone()),
            attr("lp_amount", lp_amount),
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
        reward_token_address: cfg.reward_token_address.into(),
        lp_token_address: cfg.lp_token_address.into(),
        reward_amount: cfg.reward_amount,
        lp_amount: cfg.lp_amount,
        last_time: cfg.last_time
    })
}

// settings for pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

fn query_staker(deps: Deps, address: Addr) -> StdResult<StakerResponse> {
    
    let exists = STAKERS.may_load(deps.storage, address.clone())?;
    let (mut amount, mut reward) = (Uint128::zero(), Uint128::zero());
    if exists.is_some() {
        (amount, reward) = exists.unwrap();
    } 
    Ok(StakerResponse {
        address,
        amount,
        reward
    })
}

fn map_staker(
    item: StdResult<(Addr, (Uint128, Uint128))>,
) -> StdResult<StakerInfo> {
    item.map(|(address, (amount, reward))| {
        StakerInfo {
            address,
            amount,
            reward
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
    Ok(Uint128::zero())
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

