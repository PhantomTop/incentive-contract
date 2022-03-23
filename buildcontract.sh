#!/bin/bash

#Build Flag
PARAM=$1
####################################    Constants    ##################################################

#depends on mainnet or testnet
# NODE="--node https://rpc-juno.itastakers.com:443"
# CHAIN_ID=juno-1
# DENOM="ujuno"
# FOT_ADDRESS="juno1xmpenz0ykxfy8rxr3yc3d4dtqq4dpas4zz3xl6sh873us3vajlpshzp69d"
# BFOT_ADDRESS="juno1vaeuky9hqacenay9nmuualugvv54tdhyt2wsvhnjasx9s946hhmqaq3kh7"
# GFOT_ADDRESS="juno10ynpq4wchr4ruu6mhrfh29495ep4cja5vjnkhz3j5lrgcsap9vtssyeekl"

##########################################################################################

NODE="--node https://rpc.juno.giansalex.dev:443"
#NODE="--node https://rpc.uni.junomint.com:443"
CHAIN_ID=uni-2
DENOM="ujunox"
FOT_ADDRESS="juno1yqmcu5uw27mzkacputegtg46cx55ylwgcnatjy3mejxqdjsx3kmq5a280s"
BFOT_ADDRESS="juno1f69f4902tgkuthp26ghwjwta9e5ulqdelcmdxp8acevw89w0028sflaunv"
GFOT_ADDRESS="juno18hh4dflvfdcuklc9q4ghlr83fy5k4sdx6rgfzzwhdfqznsj4xjzqdsn5cc"

##########################################################################################
#not depends
NODECHAIN=" $NODE --chain-id $CHAIN_ID"
TXFLAG=" $NODECHAIN --gas-prices 0.025$DENOM --gas auto --gas-adjustment 1.3"
WALLET="--from workshop"

WASMFILE="artifacts/gfotstaking.wasm"

FILE_UPLOADHASH="uploadtx.txt"
FILE_CONTRACT_ADDR="contractaddr.txt"
FILE_CODE_ID="code.txt"

ADDR_WORKSHOP="juno1htjut8n7jv736dhuqnad5mcydk6tf4ydeaan4s"
ADDR_ACHILLES="juno15fg4zvl8xgj3txslr56ztnyspf3jc7n9j44vhz"
ADDR_CEM="juno14u54rmpw78wux6vvrdx2vpdh998aaxxmrn6p7s"

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
    cp target/wasm32-unknown-unknown/release/gfotstaking.wasm $WASMFILE
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
    junod tx wasm instantiate $CODE_ID '{"owner":"'$ADDR_WORKSHOP'", "fot_token_address":"'$FOT_ADDRESS'","bfot_token_address":"'$BFOT_ADDRESS'", "gfot_token_address":"'$GFOT_ADDRESS'", "daily_fot_amount":"300000000000000", "apy_prefix":"109500000", "reward_interval":3600}' --label "GFOT Updated Staking" $WALLET $TXFLAG -y
}

#Get Instantiated Contract Address
GetContractAddress() {
    echo "================================================="
    echo "Get contract address by code"
    
    #read from FILE_CODE_ID
    CODE_ID=$(cat $FILE_CODE_ID)
    junod query wasm list-contract-by-code $CODE_ID $NODECHAIN --output json
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
SendFot() {
    CONTRACT_GFOTSTAKING=$(cat $FILE_CONTRACT_ADDR)
    junod tx wasm execute $FOT_ADDRESS '{"send":{"amount":"3000000000000","contract":"'$CONTRACT_GFOTSTAKING'","msg":""}}' $WALLET $TXFLAG -y
}

SendGFot() {
    CONTRACT_GFOTSTAKING=$(cat $FILE_CONTRACT_ADDR)
    junod tx wasm execute $GFOT_ADDRESS '{"send":{"amount":"224890846943","contract":"'$CONTRACT_GFOTSTAKING'","msg":""}}' $WALLET $TXFLAG -y
}

RemoveStaker() {
    CONTRACT_GFOTSTAKING=$(cat $FILE_CONTRACT_ADDR)
    junod tx wasm execute $CONTRACT_GFOTSTAKING '{"remove_staker":{"address":"'$ADDR_WORKSHOP'"}}' $WALLET $TXFLAG -y
}

RemoveAllStakers() {
    CONTRACT_GFOTSTAKING=$(cat $FILE_CONTRACT_ADDR)
    junod tx wasm execute $CONTRACT_GFOTSTAKING '{"remove_all_stakers":{}}' $WALLET $TXFLAG -y
}

WithdrawFot() {
    CONTRACT_GFOTSTAKING=$(cat $FILE_CONTRACT_ADDR)
    junod tx wasm execute $CONTRACT_GFOTSTAKING '{"withdraw_fot":{}}' $WALLET $TXFLAG -y
}

WithdrawGFot() {
    CONTRACT_GFOTSTAKING=$(cat $FILE_CONTRACT_ADDR)
    junod tx wasm execute $CONTRACT_GFOTSTAKING '{"withdraw_g_fot":{}}' $WALLET $TXFLAG -y
}

ClaimReward() {
    CONTRACT_GFOTSTAKING=$(cat $FILE_CONTRACT_ADDR)
    junod tx wasm execute $CONTRACT_GFOTSTAKING '{"claim_reward":{}}' $WALLET $TXFLAG -y
}

Unstake() {
    CONTRACT_GFOTSTAKING=$(cat $FILE_CONTRACT_ADDR)
    junod tx wasm execute $CONTRACT_GFOTSTAKING '{"unstake":{}}' $WALLET $TXFLAG -y
}

UpdateConfig() {
    CONTRACT_GFOTSTAKING=$(cat $FILE_CONTRACT_ADDR)
    junod tx wasm execute $CONTRACT_GFOTSTAKING '{"update_config":{"new_owner":"'$ADDR_CEM'"}}' $WALLET $TXFLAG -y
}

UpdateConstants() {
    CONTRACT_GFOTSTAKING=$(cat $FILE_CONTRACT_ADDR)
    junod tx wasm execute $CONTRACT_GFOTSTAKING '{"update_constants":{"daily_fot_amount":"300000000", "apy_prefix":"109500000", "reward_interval": 600}}' $WALLET $TXFLAG -y
}

PrintConfig() {
    CONTRACT_GFOTSTAKING=$(cat $FILE_CONTRACT_ADDR)
    junod query wasm contract-state smart $CONTRACT_GFOTSTAKING '{"config":{}}' $NODECHAIN
}

PrintStaker() {
    CONTRACT_GFOTSTAKING=$(cat $FILE_CONTRACT_ADDR)
    junod query wasm contract-state smart $CONTRACT_GFOTSTAKING '{"staker":{"address":"'$ADDR_WORKSHOP'"}}' $NODECHAIN
}

PrintListStakers() {
    CONTRACT_GFOTSTAKING=$(cat $FILE_CONTRACT_ADDR)
    junod query wasm contract-state smart $CONTRACT_GFOTSTAKING '{"list_stakers":{}}' $NODECHAIN
}

PrintAPY() {
    CONTRACT_GFOTSTAKING=$(cat $FILE_CONTRACT_ADDR)
    junod query wasm contract-state smart $CONTRACT_GFOTSTAKING '{"apy":{}}' $NODECHAIN
}

#################################################################################
PrintWalletBalance() {
    echo "native balance"
    echo "========================================="
    junod query bank balances $ADDR_WORKSHOP $NODECHAIN
    echo "========================================="
    echo "FOT balance"
    echo "========================================="
    junod query wasm contract-state smart $FOT_ADDRESS '{"balance":{"address":"'$ADDR_WORKSHOP'"}}' $NODECHAIN
    echo "========================================="
    echo "BFOT balance"
    echo "========================================="
    junod query wasm contract-state smart $BFOT_ADDRESS '{"balance":{"address":"'$ADDR_WORKSHOP'"}}' $NODECHAIN
    echo "========================================="
    echo "GFOT balance"
    echo "========================================="
    junod query wasm contract-state smart $GFOT_ADDRESS '{"balance":{"address":"'$ADDR_WORKSHOP'"}}' $NODECHAIN
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
sleep 10
    SendFot
# sleep 7
#     SendGFot
sleep 10
    RemoveAllStakers
# sleep 5
#     Withdraw
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

