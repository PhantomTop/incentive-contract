#!/bin/bash

#Build Flag
PARAM=$1
####################################    Constants    ##################################################

#depends on mainnet or testnet
NODE="--node https://rpc-juno.itastakers.com:443"
CHAIN_ID=juno-1
DENOM="ujuno"
#REWARD TOKEN is BLOCK
REWARD_TOKEN_ADDRESS="juno1y9rf7ql6ffwkv02hsgd4yruz23pn4w97p75e2slsnkm0mnamhzysvqnxaq"
#STAKE TOKEN is LP TOKEN for BLOCK-JUNO pool
#STAKE_TOKEN_ADDRESS="juno1jzz4t6p2dpcteqay5yt4khyha0z3u3uhghzszv7c24r7pj02scwswsygzh"

#BLOCK-MARBLE-LP
#STAKE_TOKEN_ADDRESS="juno17sf282da63uwxfht9nuzslgan4jh7qvya0k78mc9v3muuckvjndsu8dy9m"
#BLOCK-ATOM-LP
STAKE_TOKEN_ADDRESS="juno1eqj3unwgq5najlyzyjtqdjjqx9esxpmkv2gffjsvhyszasyzwejqerpwgl"
##########################################################################################

# NODE="--node https://rpc.juno.giansalex.dev:443"
# #NODE="--node https://rpc.uni.junomint.com:443"
# CHAIN_ID=uni-2
# DENOM="ujunox"
# REWARD_TOKEN_ADDRESS="juno1yqmcu5uw27mzkacputegtg46cx55ylwgcnatjy3mejxqdjsx3kmq5a280s"
# STAKE_TOKEN_ADDRESS="juno18hh4dflvfdcuklc9q4ghlr83fy5k4sdx6rgfzzwhdfqznsj4xjzqdsn5cc"

##########################################################################################
#not depends
NODECHAIN=" $NODE --chain-id $CHAIN_ID"
TXFLAG=" $NODECHAIN --gas-prices 0.0025$DENOM --gas auto --gas-adjustment 1.3"
WALLET="--from new_marble"

WASMFILE="artifacts/marbleincentive.wasm"

FILE_UPLOADHASH="uploadtx.txt"
FILE_CONTRACT_ADDR="contractaddr.txt"
FILE_CODE_ID="code.txt"

ADDR_WORKSHOP="juno1htjut8n7jv736dhuqnad5mcydk6tf4ydeaan4s"
ADDR_ACHILLES="juno15fg4zvl8xgj3txslr56ztnyspf3jc7n9j44vhz"
# ADDR_MARBLE="juno1gxlwgusm7mngml9kzlkmjw3fskekldxdsswvpy"
ADDR_MARBLE="juno1zzru8wptsc23z2lw9rvw4dq606p8fz0z6k6ggn"
# ADDR_ADMIN="juno14u54rmpw78wux6vvrdx2vpdh998aaxxmrn6p7s"
ADDR_ADMIN="juno1ddcvnnq0puupr0f3cyq77ffmk32ylaxcd3ahjg"

###################################################################################################
###################################################################################################
###################################################################################################
###################################################################################################
#Environment Functions
CreateEnv() {
    sudo apt-get update && sudo apt upgrade -y
    sudo apt-get install make build-essential gcc git jq chrony -y
    wget https://golang.org/dl/go1.17.3.linux-amd64.tar.gz
    sudo tar -C /usr/local -xzf go1.17.3.linux-amd64.tar.gz
    rm -rf go1.17.3.linux-amd64.tar.gz

    export GOROOT=/usr/local/go
    export GOPATH=$HOME/go
    export GO111MODULE=on
    export PATH=$PATH:/usr/local/go/bin:$HOME/go/bin
    
    rustup default stable
    rustup target add wasm32-unknown-unknown

    git clone https://github.com/CosmosContracts/juno
    cd juno
    git fetch
    git checkout v2.1.0
    make install

    rm -rf juno

    junod keys import workshop workshop.key

}

#Contract Functions

#Build Optimized Contracts
OptimizeBuild() {

    echo "================================================="
    echo "Optimize Build Start"
    
    docker run --rm -v "$(pwd)":/code \
        --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target \
        --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
        cosmwasm/rust-optimizer:0.12.4
}

RustBuild() {

    echo "================================================="
    echo "Rust Optimize Build Start"

    RUSTFLAGS='-C link-arg=-s' cargo wasm

    mkdir artifacts
    cp target/wasm32-unknown-unknown/release/marbleincentive.wasm $WASMFILE
}

#Writing to FILE_UPLOADHASH
Upload() {
    echo "================================================="
    echo "Upload $WASMFILE"
    
    UPLOADTX=$(junod tx wasm store $WASMFILE $WALLET $TXFLAG --output json -y | jq -r '.txhash')
    echo "Upload txHash:"$UPLOADTX
    
    #save to FILE_UPLOADHASH
    echo $UPLOADTX > $FILE_UPLOADHASH
    echo "wrote last transaction hash to $FILE_UPLOADHASH"
}

#Read code from FILE_UPLOADHASH
GetCode() {
    echo "================================================="
    echo "Get code from transaction hash written on $FILE_UPLOADHASH"
    
    #read from FILE_UPLOADHASH
    TXHASH=$(cat $FILE_UPLOADHASH)
    echo "read last transaction hash from $FILE_UPLOADHASH"
    echo $TXHASH
    
    QUERYTX="junod query tx $TXHASH $NODECHAIN --output json"
	CODE_ID=$(junod query tx $TXHASH $NODECHAIN --output json | jq -r '.logs[0].events[-1].attributes[0].value')
	echo "Contract Code_id:"$CODE_ID

    #save to FILE_CODE_ID
    echo $CODE_ID > $FILE_CODE_ID
}

#Instantiate Contract
Instantiate() {
    echo "================================================="
    echo "Instantiate Contract"
    
    #read from FILE_CODE_ID
    CODE_ID=$(cat $FILE_CODE_ID)
    junod tx wasm instantiate $CODE_ID '{"owner":"'$ADDR_MARBLE'", "reward_token_address":"'$REWARD_TOKEN_ADDRESS'", "stake_token_address":"'$STAKE_TOKEN_ADDRESS'", "daily_reward_amount":"95890000000", "apy_prefix":"10000", "reward_interval":86400, "delta_time":0, "lock_days":14, "enabled":true}' --label "BLOCK-ATOM Incentive" $WALLET $TXFLAG -y
}

#Get Instantiated Contract Address
GetContractAddress() {
    echo "================================================="
    echo "Get contract address by code"
    
    #read from FILE_CODE_ID
    CODE_ID=$(cat $FILE_CODE_ID)
    #junod query wasm list-contract-by-code $CODE_ID $NODECHAIN --output json
    CONTRACT_ADDR=$(junod query wasm list-contract-by-code $CODE_ID $NODECHAIN --output json | jq -r '.contracts[-1]')
    
    echo "Contract Address : "$CONTRACT_ADDR

    #save to FILE_CONTRACT_ADDR
    echo $CONTRACT_ADDR > $FILE_CONTRACT_ADDR
}


###################################################################################################
###################################################################################################
###################################################################################################
###################################################################################################
#Send initial tokens
SendReward() {
    CONTRACT_INCENTIVE=$(cat $FILE_CONTRACT_ADDR)
    junod tx wasm execute $REWARD_TOKEN_ADDRESS '{"send":{"amount":"5154298000000","contract":"'$CONTRACT_INCENTIVE'","msg":""}}' $WALLET $TXFLAG -y
}

SendStake() {
    CONTRACT_INCENTIVE=$(cat $FILE_CONTRACT_ADDR)
    junod tx wasm execute $STAKE_TOKEN_ADDRESS '{"send":{"amount":"1000000","contract":"'$CONTRACT_INCENTIVE'","msg":""}}' $WALLET $TXFLAG -y
}

RemoveStaker() {
    CONTRACT_INCENTIVE=$(cat $FILE_CONTRACT_ADDR)
    junod tx wasm execute $CONTRACT_INCENTIVE '{"remove_staker":{"address":"'$ADDR_MARBLE'"}}' $WALLET $TXFLAG -y
}

RemoveAllStakers() {
    CONTRACT_INCENTIVE=$(cat $FILE_CONTRACT_ADDR)
    junod tx wasm execute $CONTRACT_INCENTIVE '{"remove_all_stakers":{}}' $WALLET $TXFLAG -y
}

WithdrawReward() {
    CONTRACT_INCENTIVE=$(cat $FILE_CONTRACT_ADDR)
    junod tx wasm execute $CONTRACT_INCENTIVE '{"withdraw_reward":{}}' $WALLET $TXFLAG -y
}

WithdrawStake() {
    CONTRACT_INCENTIVE=$(cat $FILE_CONTRACT_ADDR)
    junod tx wasm execute $CONTRACT_INCENTIVE '{"withdraw_stake":{}}' $WALLET $TXFLAG -y
}

ClaimReward() {
    CONTRACT_INCENTIVE=$(cat $FILE_CONTRACT_ADDR)
    junod tx wasm execute $CONTRACT_INCENTIVE '{"claim_reward":{}}' $WALLET $TXFLAG -y
}

Unstake() {
    CONTRACT_INCENTIVE=$(cat $FILE_CONTRACT_ADDR)
    junod tx wasm execute $CONTRACT_INCENTIVE '{"create_unstake":{"unstake_amount":"100000"}}' $WALLET $TXFLAG -y
}

FetchUnstake() {
    CONTRACT_INCENTIVE=$(cat $FILE_CONTRACT_ADDR)
    junod tx wasm execute $CONTRACT_INCENTIVE '{"fetch_unstake":{"index":0}}' $WALLET $TXFLAG -y
}

UpdateConfig() {
    CONTRACT_INCENTIVE=$(cat $FILE_CONTRACT_ADDR)
    junod tx wasm execute $CONTRACT_INCENTIVE '{"update_config":{"new_owner":"'$ADDR_ADMIN'"}}' $WALLET $TXFLAG -y
}

UpdateConstants() {
    CONTRACT_INCENTIVE=$(cat $FILE_CONTRACT_ADDR)
    junod tx wasm execute $CONTRACT_INCENTIVE '{"update_constants":{"daily_reward_amount":"10000000000", "apy_prefix":"10000", "reward_interval":300, "lock_days":0,"enabled":true}}' $WALLET $TXFLAG -y
}

PrintConfig() {
    CONTRACT_INCENTIVE=$(cat $FILE_CONTRACT_ADDR)
    junod query wasm contract-state smart $CONTRACT_INCENTIVE '{"config":{}}' $NODECHAIN
}

PrintStaker() {
    CONTRACT_INCENTIVE=$(cat $FILE_CONTRACT_ADDR)
    junod query wasm contract-state smart $CONTRACT_INCENTIVE '{"staker":{"address":"'$ADDR_MARBLE'"}}' $NODECHAIN
}

PrintListStakers() {
    CONTRACT_INCENTIVE=$(cat $FILE_CONTRACT_ADDR)
    junod query wasm contract-state smart $CONTRACT_INCENTIVE '{"list_stakers":{}}' $NODECHAIN
}

PrintUnstaking() {
    CONTRACT_INCENTIVE=$(cat $FILE_CONTRACT_ADDR)
    junod query wasm contract-state smart $CONTRACT_INCENTIVE '{"unstaking":{"address":"'$ADDR_MARBLE'"}}' $NODECHAIN
}

PrintAPY() {
    CONTRACT_INCENTIVE=$(cat $FILE_CONTRACT_ADDR)
    junod query wasm contract-state smart $CONTRACT_INCENTIVE '{"apy":{}}' $NODECHAIN
}

#################################################################################
PrintWalletBalance() {
    echo "native balance"
    echo "========================================="
    junod query bank balances $ADDR_MARBLE $NODECHAIN
    echo "========================================="
    echo "BLOCK Token balance"
    echo "========================================="
    junod query wasm contract-state smart $REWARD_TOKEN_ADDRESS '{"balance":{"address":"'$ADDR_MARBLE'"}}' $NODECHAIN
    echo "========================================="
    echo "LP Token balance"
    echo "========================================="
    junod query wasm contract-state smart $STAKE_TOKEN_ADDRESS '{"balance":{"address":"'$ADDR_MARBLE'"}}' $NODECHAIN
}

#################################### End of Function ###################################################
if [[ $PARAM == "" ]]; then
    RustBuild
    Upload
sleep 12
    GetCode
sleep 12
    Instantiate
sleep 10
    GetContractAddress
# sleep 10
#     SendReward
# sleep 7
#     SendStake
sleep 10
    SendStake
sleep 5
    Unstake
sleep 5
    PrintUnstaking
sleep 5
    FetchUnstake
sleep 7
    PrintConfig
sleep 7
    PrintWalletBalance
# sleep 7
#     RemoveStaker
# sleep 5
#     PrintStaker
sleep 5
    PrintListStakers
else
    $PARAM
fi

# OptimizeBuild
# Upload
# GetCode
# Instantiate
# GetContractAddress
# CreateEscrow
# TopUp

