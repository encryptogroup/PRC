cargo build --release
SET_RTT=20 ./measurements/bench.sh
SET_RTT=40 ./measurements/bench.sh
SET_RTT=80 ./measurements/bench.sh
cd measurements
python3 PCR_plots.py