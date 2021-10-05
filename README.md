## TODO:

* unschedule a job set if it fails to build on every node
* configured singularity work binding path

```
[2021-10-03][20:37:34][distribute::server::job_pool][INFO] saving solver file to "results/hit3d/parameter_sweep_0.1,0.2/ep2-neg_no-diffusion/spectra"
[2021-10-03][20:37:34][distribute::transport][DEBUG] sending buffer of length 1
[2021-10-03][20:37:34][distribute::transport][DEBUG] reading buffer bytes w/ index 0 and buffer len 8
[2021-10-03][20:37:34][distribute::transport][DEBUG] finished reading bytes from buffer
[2021-10-03][20:37:34][distribute::transport][DEBUG] receiving buffer with length 25943
[2021-10-03][20:37:34][distribute::transport][DEBUG] reading buffer bytes w/ index 0 and buffer len 25943
[2021-10-03][20:37:34][distribute::transport][DEBUG] reading buffer bytes w/ index 17376 and buffer len 25943
[2021-10-03][20:37:34][distribute::transport][DEBUG] reading buffer bytes w/ index 20272 and buffer len 25943
[2021-10-03][20:37:34][distribute::transport][DEBUG] reading buffer bytes w/ index 21720 and buffer len 25943
[2021-10-03][20:37:34][distribute::transport][DEBUG] reading buffer bytes w/ index 23168 and buffer len 25943
[2021-10-03][20:37:34][distribute::transport][DEBUG] reading buffer bytes w/ index 24616 and buffer len 25943
[2021-10-03][20:37:34][distribute::transport][DEBUG] finished reading bytes from buffer
[2021-10-03][20:37:34][distribute::server::job_pool][INFO] saving solver file to "results/hit3d/parameter_sweep_0.1,0.2/ep2-neg_no-diffusion/f_rate_compared.png"
[2021-10-03][20:37:34][distribute::transport][DEBUG] sending buffer of length 1
[2021-10-03][20:37:34][distribute::transport][DEBUG] reading buffer bytes w/ index 0 and buffer len 8
[2021-10-03][20:37:34][distribute::transport][DEBUG] finished reading bytes from buffer
[2021-10-03][20:37:34][distribute::transport][DEBUG] receiving buffer with length 1
[2021-10-03][20:37:34][distribute::transport][DEBUG] reading buffer bytes w/ index 0 and buffer len 1
[2021-10-03][20:37:34][distribute::transport][DEBUG] finished reading bytes from buffer
[2021-10-03][20:37:34][distribute::server::job_pool][DEBUG] a node has asked for a new job
[2021-10-03][20:37:34][distribute::server::schedule][DEBUG] called finish job with 3
thread 'main' panicked at 'called `Option::unwrap()` on a `None` value', src/server/schedule.rs:99:50
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
thread 'main' panicked at 'called `Result::unwrap()` on an `Err` value: RecvError(())', src/server/job_pool.rs:379:
```
