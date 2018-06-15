#!/bin/bash
set -e;

END(){
    if test "${docker_ids}"; then
        echo "KILLING docker containers" > $OUTPUT;
        for id in ${docker_ids}
            do docker rm -f ${id} 2> $OUTPUT 1> $OUTPUT;
        done
    fi
}

IS_INTERACTIVE=false
DEBUG=${DEBUG:=false}

if [ "$1" = "--interactive" ]
then
    IS_INTERACTIVE=true
fi

OUTPUT=/dev/null

if $DEBUG
then
    OUTPUT=/dev/stdout
fi

trap 'END' EXIT;

function setup() {

    echo "Starting up ...";

    #### Env variable to run all services

    export RUST_TEST_THREADS=1;
    export BITCOIN_RPC_URL="http://localhost:18443"
    export BITCOIN_RPC_USERNAME="bitcoin"
    export BITCOIN_RPC_PASSWORD="54pLR_f7-G6is32LP-7nbhzZSbJs_2zSATtZV_r05yg="
    export ETHEREUM_NODE_ENDPOINT="http://localhost:8545"
    export ETHEREUM_NETWORK_ID=42
    export ETHEREUM_PRIVATE_KEY=3f92cbc79aa7e29c7c5f3525749fd7d90aa21938de096f1b78710befe6d8ef59

    export TREASURY_SERVICE_URL=http://localhost:8020
    export EXCHANGE_SERVICE_URL=http://localhost:8010
    export TRADING_SERVICE_URL=http://localhost:8000

    #### Start all services

    docker-compose up -d 2> $OUTPUT 1> $OUTPUT

    sleep 5;

    docker_ids=$(docker-compose ps -q)

    ########

    #### Env variables to run the end-to-end test

    export ETH_HTLC_ADDRESS="0xa00f2cac7bad9285ecfd59e8860f5b2d8622e099"

    cd "target/debug"
    cli="./trading_client"
    curl="curl -s"

    symbol_param="--symbol=ETH-BTC"
    eth_amount=10
    client_refund_address="bcrt1qcqslz7lfn34dl096t5uwurff9spen5h4v2pmap"
    client_success_address="0x03744e31a6b9e6c6f604ff5d8ce1caef1c7bb58c"
    # For contract calling
    client_sender_address="0x96984c3e77f38ed01d1c3d98f4bd7c8b11d51d7e"

    ## Generate funds and activate segwit
    $curl --user $BITCOIN_RPC_USERNAME:$BITCOIN_RPC_PASSWORD --data-binary \
    "{\"jsonrpc\": \"1.0\",\"id\":\"curltest\",\"method\":\"generate\", \"params\": [ 432 ]}" -H 'content-type: text/plain;' $BITCOIN_RPC_URL  > $OUTPUT

    # Watch the pw2sh address
    $curl --user $BITCOIN_RPC_USERNAME:$BITCOIN_RPC_PASSWORD --data-binary \
    "{\
        \"jsonrpc\": \"1.0\",\
        \"id\":\"curltest\",\
        \"method\": \"importaddress\",\
        \"params\":\
            [\
                \"bcrt1qcqslz7lfn34dl096t5uwurff9spen5h4v2pmap\",\
                \"htlc\"\
            ]\
    }" \
    -H 'content-type: text/plain;' $BITCOIN_RPC_URL > $OUTPUT

    echo "System is ready!"
}

function print_green() {
    printf '\e[32m%s\e[0m\n' "$1"
}

function print_blue() {
    printf '\e[34m%s\e[0m\n' "$1"
}

function new_offer() {
    ## Offer
    cmd="$cli offer ${symbol_param} --amount=${eth_amount} buy"
    print_green "$cmd"
    output=$($cmd)
    echo "$output"

    ## get UID
    uid=$(echo "$output" | head -n1 | grep "Trade id" |sed 's/^.* Trade id: \(.*\) .*$/\1/')
    # echo "--> Trade id: ${uid}"
}

function new_order() {

    cmd="$cli order ${symbol_param} --uid=${uid} --refund-address=${client_refund_address} --success-address=${client_success_address}"
    print_green "$cmd"
    output=$($cmd)
    echo "$output"

    ## Get BTC HTLC address
    btc_htlc_address=$(echo "$output" | grep "^bcrt1")
    # echo "--> BTC HTLC: ${btc_htlc_address}"

    ## Get BTC amount
    btc_amount=$(echo "$output" | grep "Please send" | sed -E 's/^Please send ([0-9\.]+) BTC.*$/\1/')
    # echo "--> BTC amount: ${btc_amount}"
}

function generate_blocks() {

    ## Generate blocks to confirm the transaction
    $curl --user $BITCOIN_RPC_USERNAME:$BITCOIN_RPC_PASSWORD --data-binary \
    "{\"jsonrpc\": \"1.0\",\"id\":\"curltest\",\"method\":\"generate\", \"params\": [ 6 ]}" -H 'content-type: text/plain;' $BITCOIN_RPC_URL > $OUTPUT

}
function fund_htlc() {

    ## Bitcoin RPC call
    output=$($curl --user $BITCOIN_RPC_USERNAME:$BITCOIN_RPC_PASSWORD --data-binary \
    "{\"jsonrpc\": \"1.0\",\"id\":\"curltest\",\"method\":\"sendtoaddress\", \"params\": [ \"${btc_htlc_address}\", ${btc_amount}]}" -H 'content-type: text/plain;' $BITCOIN_RPC_URL)

    ## Get funding tx id
    htlc_funding_tx=$(echo $output | sed -E 's/^..result.:.([a-z0-9]+).,.error.*$/\1/')

    generate_blocks;

    ## Get raw funding tx
    output=$($curl --user $BITCOIN_RPC_USERNAME:$BITCOIN_RPC_PASSWORD --data-binary \
    "{\"jsonrpc\": \"1.0\",\"id\":\"curltest\",\"method\":\"getrawtransaction\", \"params\": [ \"${htlc_funding_tx}\" ]}" \
    -H 'content-type: text/plain;' $BITCOIN_RPC_URL)

    raw_funding_tx=$(echo $output | sed -E 's/^..result.:.([a-z0-9]+).,.error.*$/\1/')

    ## Decode raw funding tx
    output=$($curl --user $BITCOIN_RPC_USERNAME:$BITCOIN_RPC_PASSWORD --data-binary \
    "{\"jsonrpc\": \"1.0\",\"id\":\"curltest\",\"method\":\"decoderawtransaction\", \"params\": [ \"${raw_funding_tx}\" ]}"\
     -H 'content-type: text/plain;' $BITCOIN_RPC_URL)

    ## Getting the vout which pays the BTC HTLC
    htlc_funding_tx_vout=$(echo $output | jq .result.vout | jq ".[] | select(.scriptPubKey.addresses[0] == \"${btc_htlc_address}\")"|jq .n)

    echo "HTLC successfully funded - BTC payment was made."
}

function notify_exchange_service_btc_htlc_funded() {

    result=$($curl --data-binary "{\"transaction_id\": \"${htlc_funding_tx}\",\"vout\": ${htlc_funding_tx_vout}}" -H 'Content-Type: application/json' ${EXCHANGE_SERVICE_URL}/trades/ETH-BTC/${uid}/buy-order-htlc-funded )

    echo $result > $OUTPUT

    print_blue "Notified exchange about trader's BTC payment (Trader funded BTC HTLC)."
}

function notify_trading_service_eth_htlc_funded() {

    result=$($curl --data-binary "{\"contract_address\": \"${ETH_HTLC_ADDRESS}\"}" -H 'Content-Type: application/json' ${TRADING_SERVICE_URL}/trades/ETH-BTC/${uid}/buy-order-contract-deployed)

    echo $result > $OUTPUT

    print_blue "Notified trader about exchange's ETH payment (Exchange funded ETH HTLC)."
}

function notify_exchange_service_eth_redeemed() {
    $curl --data-binary "{\"secret\": \"${secret}\"}" -H 'Content-Type: application/json' ${EXCHANGE_SERVICE_URL}/trades/ETH-BTC/${uid}/buy-order-secret-revealed > $OUTPUT

    print_blue "Notified exchange about revealed secret (Trader redeemed ETH funds)."
}
function get_redeem_details() {

    cmd="$cli redeem ${symbol_param} --uid=${uid}"

    print_green "$cmd"

    output=$($cmd)

    secret=$(echo "$output" | tail -n1 |sed -E 's/^ethereum:.*bytes32=(.+)$/\1/')

    echo "Secret: $secret"
}

function get_eth_balance() {

    output=$($curl --data-binary "{\
      \"jsonrpc\":\"2.0\",\
      \"method\":\"eth_getBalance\",\
      \"params\":[\
        \"${client_success_address}\",\
        \"latest\"\
      ],\
      \"id\":1\
    }" \
    -H 'Content-Type: application/json' ${ETHEREUM_NODE_ENDPOINT})

    echo $output|jq -r .result
}

function redeem_eth() {
    $curl --data-binary "{\
      \"jsonrpc\":\"2.0\",\
      \"method\":\"eth_sendTransaction\",\
      \"params\":[\
        {\
          \"from\": \"${client_sender_address}\",\
          \"to\": \"${ETH_HTLC_ADDRESS}\",\
          \"gas\": \"0x100000\",\
          \"gasPrice\": \"0x01\",\
          \"value\": \"0x0\",\
          \"data\": \"0x${secret}\"\
        }\
      ],\
      \"id\":1\
    }" \
    -H 'Content-Type: application/json' ${ETHEREUM_NODE_ENDPOINT} > $OUTPUT
}

function list_unspent_transactions() {
    output=$($curl --user $BITCOIN_RPC_USERNAME:$BITCOIN_RPC_PASSWORD --data-binary \
    "{\
      \"jsonrpc\":\"1.0\",\
      \"id\":\"curltest\",\
      \"method\":\"listunspent\",\
      \"params\":\
      [\
        0,\
        9999999,\
        [\
          \"bcrt1qcqslz7lfn34dl096t5uwurff9spen5h4v2pmap\"\
        ]\
      ],\
      \"id\":1\
    }" \
    -H 'content-type: text/plain;' $BITCOIN_RPC_URL)

    echo $output
}

function hex_to_dec() {
    perl -Mbigint -E 'say hex(shift)' $1
}

function is_greater_than() {
    perl -Mbigint -E 'exit !(((shift) - (shift)) > 0) ? 0 : 1' $1 $2
}

function wei_to_eth() {
    perl -Mbigint -E 'say ((shift) / 1_000_000_000_000_000_000)' $1
}

#### Start End to end test

setup;

$IS_INTERACTIVE && read;

new_offer;

$IS_INTERACTIVE && read;

new_order;

$IS_INTERACTIVE && read;

fund_htlc;

$IS_INTERACTIVE && read;

notify_exchange_service_btc_htlc_funded;

$IS_INTERACTIVE && read;

notify_trading_service_eth_htlc_funded;

$IS_INTERACTIVE && read;

get_redeem_details;

old_balance=$(get_eth_balance)
echo "Previous ETH balance in HEX: $old_balance" > $OUTPUT

old_balance=$(hex_to_dec $old_balance)
old_balance=$(wei_to_eth $old_balance)

echo "Previous ETH balance: $old_balance"

$IS_INTERACTIVE && read;

redeem_eth;

new_balance=$(get_eth_balance)
echo "New ETH balance in HEX: $new_balance" > $OUTPUT
new_balance=$(hex_to_dec $new_balance)
new_balance=$(wei_to_eth $new_balance)
echo "New ETH balance:      $new_balance"

if [ ${old_balance} -lt ${new_balance} ]
then
    echo "## ETH WAS redeemed ##" > $OUTPUT
else
    echo "## ETH was NOT redeemed ##" > $OUTPUT
    exit 1
fi

$IS_INTERACTIVE && read;

output=$(list_unspent_transactions)
old_unspent=$(echo $output |jq .result)
old_unspent_num=$(echo $output | jq '.result | length')
echo -e "BTC: Total UTXOs before redeem: $old_unspent_num"

$IS_INTERACTIVE && read;

# Poke exchange service to redeem BTC
notify_exchange_service_eth_redeemed;

generate_blocks;

# Check BTC unspent outputs after redeem
output=$(list_unspent_transactions)

new_unspent=$(echo $output |jq .result)
new_unspent_num=$(echo $output | jq '.result | length')
echo -e "BTC: Total UTXOs after redeem: $new_unspent_num"
echo -e "BTC: Amount: $(echo $new_unspent | jq '.[0].amount')"

if [ ${old_unspent_num} -lt ${new_unspent_num} ]
then
    echo "## BTC WAS redeemed ##" > $OUTPUT
else
    echo "## BTC was NOT redeemed ##" $OUTPUT
    exit 1
fi