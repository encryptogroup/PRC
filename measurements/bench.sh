#!/bin/bash
set -euo pipefail


####################################
########  Set input params  ########
####################################

# Define the database sizes
declare -a db_sizes=(128 256 512 1024 2048 4096 8192 16384 32768)
# Define party numbers
declare -a party_nums=(2 4 8)
# Number of times each test is repeated
rep=5
# Output file
binary_path="target/release/priv-rec-cert"

####################################
########  Set network delay ########
####################################
# Set network rtt in milliseconds
# Default option does 
# Add SET_RTT={20, 40, 80} as an environment variable before running the command
SET_RTT="${SET_RTT:-0}" 
delay=$(( SET_RTT / 2))
# Commands for setting network delay
tc_cmd() {
    if [[ "$(id -u)" -eq 0 ]]; then
        echo "tc : Setting RTT to $SET_RTT"
        tc "$@"
    elif command -v sudo >/dev/null 2>&1 ; then #
        if sudo sh -c 'command -v tc >/dev/null 2>&1'; then
            echo "sudo tc:  Setting RTT to $SET_RTT"
            sudo tc "$@"
        fi
    else
        echo -e "Error: tc requires root privileges.\n\tIf running inside a docker: add --cap-add=NET_ADMIN\n\tIf running outside docker, make sure iproute2 is installed and running as root." >&2
        exit 1
    fi
}
clean_net(){
    # Clear any existing delay
    echo "Cleaning existing network delays"
    tc_cmd qdisc del dev lo root || true
}
if [[ $SET_RTT -gt 0 ]] then
    # Set desired delay
    clean_net
    echo "Setting RTT to $SET_RTT"
    tc_cmd qdisc add dev lo root netem delay "${delay}ms"
fi


####################################
########  Set output file   ########
####################################
# if SET_APPEND_BENCH=0 (or not defined): then initialize an output csv (clearing existing measurements)
# otherwise SET_APPEND_BENCH: append measurements to the existing csv file
SET_APPEND_BENCH="${SET_APPEND_BENCH:-0}" 
# output_file="measurements/test_rtt_${SET_RTT}ms.csv"
output_file="measurements/measure_delay_tcp_rtt_${SET_RTT}ms.csv"
if [[ $SET_APPEND_BENCH -eq 0 ]] then
    echo "Initializing an empty measurement sheet"
    echo "party id,num parties,db l1,db l2,wall time,comp time,upload (byte),download (byte)" > $output_file
fi 
echo "output file is => $output_file"


####################################
########   Run experiments  ########
####################################
# Loop through the party numbers
for party_num in "${party_nums[@]}"; do
    # Loop through the database sizes
    for db in "${db_sizes[@]}"; do
        for ((id=0; id<party_num; id++)); do
            echo "Running with party_num=$party_num, id=$id, and db=$db"
            "$binary_path" --id "$id" --party-num="$party_num" --db-l1="$db" --db-l2="$db" --out-fs="$output_file" --rep-num="$rep" &
        done
        # Wait for the background processes to finish before starting the next size
        # make sure network is closed
        wait 
        sleep 0.2

        for ((id=0; id<party_num; id++)); do
            # Second run where db-l2 is half of db-l1
            db_l1="$db"
            db_l2=$((db_l1 / 2))
            echo "Running with party_num=$party_num, id=$id, db-l1=$db_l1, and db-l2=$db_l2"
            "$binary_path" --id "$id" --party-num="$party_num" --db-l1="$db_l1" --db-l2="$db_l2" --out-fs="$output_file" --rep-num="$rep" &
        done
        wait 
        sleep 0.2
    done
done

# remove network delay
if [[ $SET_RTT -gt 0 ]] then
    clean_net
fi