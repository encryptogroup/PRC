# Benchmarks
This directory includes raw measurements used in the paper, scripts to benchmark our artifact at scale, and plot performance.

We also provide a detailed description of how we evaluate other solutions, such as naive PIR and ALLOSAUR, without offering any automation to gather measurements from prior work.

## Comparison with naive PIR solutions
We will go through the necessary steps to reproduce the measurements below, both for the hintless and the dpf based approach.

### Hintless
The hintless repository used can be found [here](https://github.com/google/hintless_pir).

The benchmarks require a local installation of bazel and gcc. We use version 8.5.0 and g++-12. The benchmark file we ran is located under `hintless_simplepir/hintless_simplepir_benchmarks.cc`. We modify the table sizes, i.e., the number of rows (line 41&31) and columns (line 42&32) in this file to produce the benchmarks, attempting to choose row and column numbers as evenly as possible, e.g., we choose $2^{10}$ rows and $2^{10}$ columns for DB sizes of $2^{20}$. For odd exponents, we choose a larger number of columns than rows. 

We further set the number of bits per db entry (line 43) to be $768 = 8\cdot 96B$. For the baseline approach, each record contains a signature (Schnorr: 64B) and commitment (32B). Further, we choose the number of rows per block (line 56) to equal the number of rows.

The benchmarks can then be run using 

```bash
 bazel run -c opt //hintless_simplepir:hintless_simplepir_benchmarks -- --benchmark_filter=".*" --benchmark_time_unit=ms
 ```

 to further stabilize the benchmark results, one can use `setarch $(uname -m) -R` as a prefix to the command to disable ASLR.

#### Measurements
We obtain the following results (plotted in the paper).
| DB Size (N) | Time (ms) |
|-------------|-----------|
| $2^{13}$ | 112 |
| $2^{14}$ | 156 |
| $2^{15}$ | 156 |
| $2^{16}$ | 280 |
| $2^{17}$ | 324 |
| $2^{18}$ | 521 |
| $2^{19}$ | 515 |
| $2^{20}$ | 1,019 |
| $2^{21}$ | 1,043 |
| $2^{22}$ | 1,993 |
| $2^{23}$ | 2,004 |
| $2^{24}$ | 3,835 |
| $2^{25}$ | 3,889 |

### DPF
We used the DPF based PIR implementation [here](https://github.com/google/distributed_point_functions).

Running the benchmarks requires a functioning installation of bazel (we use version 8.5.0) and gcc (g++-12). To get the tests to run, we add the following parameters to .bazelrc: 

```bash
cat >> .bazelrc << 'EOF'
build --features=-layering_check
build --features=-parse_headers
build --incompatible_sandbox_hermetic_tmp=false
build --action_env=PATH=/usr/bin:/bin:/usr/local/bin
build --action_env=CC=/usr/bin/gcc-12
build --action_env=CXX=/usr/bin/g++-12
build --spawn_strategy=local
build --copt=-Wno-discarded-qualifiers
build --cxxopt=-Wno-discarded-qualifiers
EOF
```

Then, the pir system can be benchmarked by running the file in 'pir/dense_dpf_pir_server_benchmark.cc' using 

```bash
bazel run -c opt //pir:dense_dpf_pir_server_benchmark -- --benchmark_filter=".*" --benchmark_time_unit=ms
```

By changing lines 34 and 36 (pre-set flags), we can alter the number of records in the database and the byte size of each record. For the PIR baseline, we use record sizes of 96 bytes (64 byte Schnorr signature + 32 byte commitment).

#### Measurements
We obtain the following results (plotted in the paper)
| DB Size (N) | Time (ms) |
|-------------|-----------|
| $2^{13}$ | 0.28 |
| $2^{14}$ | 0.55 |
| $2^{15}$ | 1.10 |
| $2^{16}$ | 2.26 |
| $2^{17}$ | 5.99 |
| $2^{18}$ | 12.45 |
| $2^{19}$ | 24.04 |
| $2^{20}$ | 50.65 |
| $2^{21}$ | 95.77 |
| $2^{22}$ | 208.60 |
| $2^{23}$ | 394.47 |
| $2^{24}$ | 781.04 |
| $2^{25}$ | 1,559.40 |
| $2^{26}$ | 3,890.53 |

### Extrapolation
Since the benchmarks do not run with arbitrarily large databases on our system (simply crashing), we have extrapolated the measurements above for database sizes up to $2^{30}$. We have done so using numpy's polyfit function. Expecting that PIR runtimes scale linearly with the database size (and therefore exponentially with the logarithmically scaled x-Axis), we believe this estimation to be fairly accurate. In full detail, we use the following function to compute extrapolated values:

```python
def extrapolate_exponential(data, n_extra=2):
    """
 Extrapolate n_extra points assuming exponential growth.
 Fits log(data) vs index, then predicts further points.
 """
 x = np.arange(len(data))
 y = np.log(data)
    
    # Fit a line in log space
 coeffs = np.polyfit(x, y, 1)
 slope, intercept = coeffs
    
    # Predict next n_extra points
 x_extra = np.arange(len(data), len(data) + n_extra)
 y_extra = slope * x_extra + intercept
 extra_vals = np.exp(y_extra)
    
    return list(data) + list(extra_vals)

```

### Comparison with ALLOSAUR 

The ALLOSAUR benchmarks were performed for the available implementation [sam-jaques/allosaurust](https://github.com/sam-jaques/allosaurust).

The benchmarks are located in the `benches` directory of the repository, where we have made the following adaptations to the configuration in the code in `benches/updates.rs`:

- Set `const USERS` to a sufficiently large number of total users/credentials, i.e., `2100000`.
- Set `const SHARES` and `const THRESHOLD` to the same number of servers as for the PRC benchmarks, i.e., `2` for a 2-server setup.
- Set `const ALLOSAUR_CHANGES` to an array for the respective number of updates, i.e., revocations, we want to benchmark. For instance, `[2097152]` for 2^21 revocations.

Run the benchmarks for the accumulator update using the command:

```bash 
cargo bench --bench updates allosaur_update
```

#### Measurements

The table below lists our measured update times of ALLOSAUR depending on the number of revocations.

| #Revocations (r) | Time (s) |
|------------------|----------|
| $2^{12}$  | 0.911    |
| $2^{13}$  | 1.845    |
| $2^{15}$  | 7.558    |
| $2^{18}$  | 64.993   |
| $2^{19}$  | 130.7    |
| $2^{21}$  | 606.23  |

As we weren't able to run benchmarks for over $2^{21}$ revocations, we used a linear estimate to extrapolate for higher numbers of revocations. 