# Private Record Certifier
This repository contains a prototype implementation of **Private Record Certification (PRC)**, a primitive introduced in:

> Kasra EdalatNejad, Sebastian Faust, Jonas Hofmann, Philipp-Florens Lehwalder, Thomas Schneider.  
> *Do You Need a Receipt? Anonymous Credential Revocation at Continental Scale via Private Record Certification*.  
> USENIX Security 2026.
>
> **Abstract:** 
> A key challenge in digital credential systems is revocation, that is, the ability to invalidate credentials post-issuance and verify their status upon presentation.
While anonymous credentials enhance privacy over classical credentials (e.g., by providing unlinkability), they complicate revocation.
Existing revocation schemes for anonymous credentials often suffer from high client or verifier computation, long delays before revocation takes effect (e.g., epoch-based settings), or require updates to all users with each revocation.
> We present an efficient, real-time revocation system for anonymous credentials with decentralized revocation authorities based on a novel primitive called *Private Record Certification (PRC)*.
PRC enables users to obtain a certificate for a record stored in a server-managed database without the servers learning which record was requested. This primitive is of independent interest, and we construct it by combining techniques from private information retrieval and secure multi-party computation.
Our revocation scheme outsources its computation to the revocation authorities and has minimal overhead for clients and verifiers, while ensuring the communication costs are sublinear in the number of credentials.
We build a prototype and demonstrate that our system achieves sub-second real-time latency for PRC requests, at a scale of over *1 billion* credentials, with an online operational cost of 2.5$ per server for processing *1 million* PRC requests.

**⚠️ Warning:** This is an academic research prototype. It has not been hardened or audited and is not suitable for production use.

## 📚 Table of contents
- [⏱️ Reproduction instructions](#%EF%B8%8F-reproduction-instructions)
- [🔲 Hardware requirements](#-hardware-requirements)
- [🐋 Running benchmarks with Docker](#-running-benchmarks-with-docker)
- [🛠️ Building from source code](#%EF%B8%8F-building-from-source-code)
  - [Requirements](#requirements)
  - [Compiling](#compiling)
- [🏃 Running](#-running)
  - [Running with scripts](#running-with-scripts)
  - [Running servers manually](#running-servers-manually)
- [📁 Repository structure](#-repository-structure)
- [🔒 Known issues and limitations](#-known-issues-and-limitations)
- [📊 Reproducing the paper's claim](#-reproducing-the-papers-claim)

# ⏱️ Reproduction instructions
We provide a Docker container to automate the benchmarking process. Before running the artifact, please make sure your machine satisfies the [🔲 Hardware requirements](#-hardware-requirements), in particular AVX2 support.
The main reproduction path is described in [🐋 Running benchmarks with Docker](#-running-benchmarks-with-docker). The generated output and its relation to the paper's claims are described in [📊 Reproducing the paper's claim](#-reproducing-the-papers-claim).

**Time:** Assuming Docker is already installed, reproducing the benchmark should take about 5 minutes. The exact runtime may vary depending on the host hardware and network bandwidth.

Approximate breakdown:
* Building the Docker image: ~1 minute
* Running all experiments inside Docker: <5 minutes
* Human interaction: minimal; reproduction is fully automated and requires only two commands


# 🔲 Hardware requirements
This prototype requires an `x86_64` CPU with AVX2 support. This includes most modern Intel and AMD CPUs, but excludes ARM-based platforms, including Apple Silicon.
The repository contains server-side code for operating PRC and revocation-authority servers, and we do not plan to support non-AVX2 architectures.

Our benchmark evaluates up to 8 PRC servers. We measure single-core performance and restrict each server to one core. To simplify the process, we run all servers on the same machine with simulated network. Therefore, we recommend running the full benchmark on a machine with at least 8 cores. If fewer cores are available, multiple servers will share cores, which may increase latency.
The RAM footprint is low. A machine with 8 GiB of RAM should be sufficient for running the full 8-server benchmark.



# 🐋 Running benchmarks with Docker
The recommended way to reproduce the paper benchmarks is to use the provided Docker setup.

Install Docker using [the official instructions](https://docs.docker.com/get-docker/).
To simplify reproducing the results, we run all benchmark parties inside one Docker container on a single machine. Network delay between parties is simulated using `tc`.

Build the image:

```bash
$ sudo docker build -t prc .
$ sudo docker run \
    --cap-add=NET_ADMIN \
    --rm \
    -v "$(pwd)/measurements:/AnonCred/measurements" \
    prc \
    bash /AnonCred/bench_run_all.sh    
```

The command above:
 - Grants the container `NET_ADMIN` so that `tc` can configure simulated network delay.
 - Mounts the local `measurements/` directory into the container so results persist after the container exits.
 - Runs the master benchmark script `bench_run_all.sh`;
 - Stores raw measurements and plots in `measurements/`;
 - Removes the container after completion because of `--rm`.

After the benchmark finishes, the following measurements and plots should be generated:

```
measurements/latency.pdf
measurements/costs.pdf
measurements/comp_comm.pdf
measurements/measure_delay_tcp_rtt_20ms.csv
measurements/measure_delay_tcp_rtt_40ms.csv
measurements/measure_delay_tcp_rtt_80ms.csv
```

These correspond to the measurements reported in Figure 3 of the paper.

Note: When all parties run on one machine, latency may increase if the number of parties exceeds the number of available CPU cores or hardware threads.


# 🛠️ Building from source code
We provide detailed instructions on how to build and run our prototype here. These instruction are based on Debian/Ubuntu systems and you may need to adjust commands if you want to manually run our prototype on other systems outside a Docker environment.

## Requirements
The implementation is written in Rust. If Rust is not installed, follow the [official installation instructions](https://rust-lang.org/tools/install/).

For benchmarking, we also require `iproute2`, which provides the `tc` command used for network shaping. This is not required for compilation.

The plotting scripts use Python and Matplotlib. If LaTeX is available, the scripts use it for nicer plot labels. Otherwise, they automatically fall back to non-LaTeX labels to avoid requiring a large TeX Live installation.

On Debian/Ubuntu systems, install the dependencies with:

```bash
$ sudo apt install iproute2 python3 python3-matplotlib
```


## Compiling
**Test**
After installing the requirements, run the test suite:

```bash
$ cargo test
```

All tests should pass.


**Building**
For benchmarking, build the project in release mode:

```bash
$ cargo build --release
```

Using `--release` is important for obtaining performance numbers.

# 🏃 Running 
You can either use the provided scripts to launch multiple servers or run each server process manually.

## Running with scripts

The benchmark scripts use the following environment variables:
 - `SET_RTT`: simulates round-trip time in milliseconds, e.g., SET_RTT=20 will apply `tc` with a one-way delay of 10ms. If this variable is not set, the script does not change the network configuration.
 - `RUST_LOG`: controls logging. Setting `RUST_LOG=info` prints benchmark progress and detailed interaction costs. Otherwise, the server runs quietly without output.
 - `SET_APPEND_BENCH`: controls whether benchmark results are appended. If unset, the output CSV file is cleared and a new header is written. If set to 1, new measurements are appended to the existing file.

*Example:* 
The following command runs the PRC protocol with 2, 4, and 8 parties, for database sizes from 2^14 to 2^30 records, using a simulated round-trip time of 20 ms.

```bash
SET_RTT=20 ./measurements/bench.sh
```

*Note:* When running the benchmark scripts outside a Docker with `NET_ADMIN` while having set `SET_RTT`, the script requires `sudo` to run `tc`. 


## Running servers manually
The main executable is `./target/release/priv-rec-cert`. Use `--help` to list all options:


```bash
$ ./target/release/priv-rec-cert --help
Runs a multiparty credential non-revocation check protocol

Usage: priv-rec-cert [OPTIONS] --id <ID> --out-fs <OUT_FS>

Options:
      --id <ID>                Peer ID
      --party-num <PARTY_NUM>  Number of parties [default: 2]
      --db-l1 <DB_L1>          DB's 1st dimension size. Must be a power of 2 and larger or equal to db_l2 [default: 1024]
      --db-l2 <DB_L2>          DB's 2nd dimension size. Must be a power of 2 [default: 1024]
      --rep-num <REP_NUM>      Number of repetitions [default: 10]
      --base-port <BASE_PORT>  Base port for the server. ports will be assigned sequentially starting from this port [default: 8000]
      --out-fs <OUT_FS>        Append the measurement results to the output file
  -h, --help                   Print help
  -V, --version                Print version
```

To run the protocol with n parties, start n processes with `--party-num n` and party IDs `--id` from 0 to n-1.

All parties run on localhost. Ports are assigned sequentially starting from `--base-port`. Parties listens on `127.0.0.1:($base-port + $id)`.


*Example:* to run a two-party setting for a database with 2^24 records, start the two parties in separate terminals:

```bash
$ RUST_LOG=info ./target/release/priv-rec-cert \
    --id 0 \
    --party-num=2 \
    --db-l1=4096 \
    --db-l2=4096 \
    --out-fs=test.csv \
    --rep-num=5
```

```bash
$ ./target/release/priv-rec-cert \
    --id 1 \
    --party-num=2 \
    --db-l1=4096 \
    --db-l2=4096 \
    --out-fs=test.csv \
    --rep-num=5
```

This runs 5 PRC queries (`--rep-num`) on a random database of 16M records handled by two servers. The executable writes measurements to the specified CSV file. By default, it produces no terminal output. Set `RUST_LOG=info` to print progress and costs. 


# 📁 Repository structure
The repository is organized around three main components: correlated randomness, SIMD-friendly bit arrays, and the PRC protocol implementation.

- **`cor_rnd`**: This module contains the secret-sharing and preprocessing utilities used by the MPC layer.
  - *`cor_rnd/sharing.rs`*: Defines the secret-sharing abstractions and share operations.
  - *`cor_rnd/beaver.rs`*: Implements Beaver triplets, which are used for secure AND/multiplication on secret-shared values.
  - *`cor_rnd/dabit.rs`*: Implements daBits, which are used for conversions between Boolean and arithmetic secret sharing.

- **`simd_array`**: This module provides compact bit-array data structures with memory alignment suitable for AVX2 operations.
  - *`simd_array/aligned_array.rs`*: Provides aligned memory allocation and storage utilities for SIMD-friendly data structures.
  - *`simd_array/bit_array.rs`*: Implements compact one-dimensional bit arrays with bit-level access.
  - *`simd_array/d2_bit_array.rs`*: Implements compact two-dimensional bit arrays and matrices used for database and selector operations.

- **`prc`**: This module contains the main PRC protocol implementation, including networking, database handling, MPC execution, boolean to OHE share conversion, and server logic.
  - *`prc/b2a_conv.rs`*: Implements Boolean to arithmetic share-conversion.
  - *`prc/client.rs`*: Implements client-side query generation.
  - *`prc/commitment.rs`*: Implements Pedersen commitment.
  - *`prc/config.rs`*: Provides configuration parameters for the peers and database.
  - *`prc/connection.rs`*: Handles message and command passing between PRC parties. Support TCP connections and local Rust async channels.
  - *`prc/db.rs`*: Defines the server-side database representation.
  - *`prc/layer.rs`*: Implements an abstraction for the MPC AND/multiplication layer, facilitating building circuits and executing them layer by layer.
  - *`prc/mpc.rs`*: Provides a custom AVX2-accelerated MPC engine.
  - *`prc/server.rs`*: Implements PRC server behavior and coordinates protocol execution.
  - *`prc/util.rs`*: Provides shared helper functions used across the PRC implementation.

-------
Besides the modules, we also provide:

- **Entry points**:
  * *`lib.rs`*: Defines the library entry point and exposes the main modules.
  * *`main.rs`*: Defines the command-line executable used to run PRC servers and artifact benchmarks.

- **`measurements`**: Contains the raw benchmark data and plotting scripts used to generate the evaluation figures.
  - *`measurements/raw data/`*: Contains the raw measurements used to generate the plots in the paper.
  - *`measurements/bench.sh`*: An script to run n PRC servers and gather measurements.
  - *`measurements/PRC_plots.py`*: Generates the PRC benchmark plots from the raw measurements (Figure 3).
  - *`measurements/PIR_comparison_plot.py`*: Generates the PIR-baseline comparison plot (Figure 4).


# 🔒 Known issues and limitations
This repository prioritizes reproducibility and benchmarking. Some implementation choices simplify artifact evaluation but are not secure for deployment. This repository is an academic research prototype. It has not been hardened or audited and is not suitable for production use.

**Secure connection:** the prototype currently uses TCP connections. The PRC construction requires secure channels between parties. We omit TLS in the prototype to avoid certificate and credential management during benchmarking. A production deployment must use authenticated secure channels, such as TLS.

**Insecure correlated-randomness setup:** the prototype uses a seed-based dealer to generate preprocessing material. However, instead of running the dealer, we let all parties know the dealer's seed and allow parties to learn all secret shares. This produces the required material while skipping the pre-processing phase. This is insecure and is only used to simplify reproducibility. In the paper, we discuss alternatives depending on the desired preprocessing cost and threat model.  Alternatives include generating the preprocessing material via standard dealer protocols, trusted execution environments, or maliciously secure correlated-randomness generation.

The protocol only requires correlated randomness sublinear to the input size, specifically for $N$ l-bit records, our protocol requires $3\sqrt{N}$ Beaver triplets and $\lg{N} + l$ daBits. Moreover, preprocessing does not affect the online costs reported in the paper.

# 📊 Reproducing the paper's claim
This artifact reproduces the server-side PRC measurements shown in Figure 3 of the paper. The benchmark directly measures latency, computation cost, and communication cost.
The estimated monetary cost is computed from measured resource consumption using AWS pricing. Our pricing model is as follows: one CPU core, less than 2 GiB RAM, $0.04 per compute hour, free inbound transfer, and $0.09 per GiB outbound.

Running the Docker benchmark produces the following files:
```
measurements/latency.pdf
measurements/costs.pdf
measurements/comp_comm.pdf
```

These plots should match the corresponding Figure 3 results in the paper and support the following claims:
 - *Sub-second latency at billion-credential scale.* PRC achieves sub-second latency even for databases with over one billion credentials. The latency is dominated by the simulated network delay.
 - *Linear server computation.* The computation cost of PRC servers grows linearly with the number of credentials.
 - *Sublinear server communication.* The communication cost of PRC servers grows sublinearly with the number of credentials; more precisely it grows as $O(\sqrt{N})$.
 - *Efficient monetary cost at scale.* The monetary cost grows linearly with the number of credentials, and the protocol remains efficient even when processing millions of PRC queries.


**Claims not directly shown in Figure 3:**
The paper also makes the following client-side claims, which are not directly shown in Figure 3.
 - *Client cost is independent of the number of revocations.* In our protocol, revoking a credential only requires changing a value in the servers' database. There is no corresponding client-side update, and client queries are agnostic to the number of revocations performed by the servers.
 - *Client cost is logarithmic to the number of credentials.* In theory, the client cost is logarithmic in the number of credentials because the record index size grows with the database size. In practice, we represent indices as integers where a `u32` supports up to four billion records. Therefore, within the evaluated range, the client cost is effectively independent of the number of credentials.


**Out of scope:** 
This artifact does not fully reproduce Figure 4 end-to-end: the paper's comparison relies on external related-work implementations, which are not repackaged in this repository. We instead provide our measurements, the recorded comparison data, and `measurements/PIR_comparison_plot.py`, allowing reviewers to regenerate the plot but not rerun the external systems.

## AI use
We have used LLM-based tools mainly for polishing, refactoring, and expanding test coverage. The output of AI tools has been manually checked and verified.