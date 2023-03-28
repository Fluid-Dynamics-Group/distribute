# Execution Scripts

Execution scripts are specified in the `file` key of a list item a job `name` in `jobs`. Execution scripts
can do a lot of things. I have found it productive to write a single `generic_run.py` script that
reads a configuration file from `./input/input.json` is spefied under my `required_files` for the job)
and then run the sovler from there. 

One import thing about execution scripts is that they are run with a command line argument specifying
how many cores you are allowed to use. If you hardcode the number of cores you use you will either
oversaturate the processor (therefore slowing down the overall execution speed), or undersaturate 
the resources available on the machine. Your script will be "executed" as if it was a command line 
program. If the computer had 16 cores available, this would be the command:

```bash
python3 ./job.py 16
```

you can parse this value using the `sys.argv` value in your script:

```python
import sys
allowed_processors = sys.argv[1]
allowed_processors_int = int(allowed_processors)
assert(allowed_processors_int, 16)
```

**You must ensure that you use all available cores on the machine**. If your code can only use a reduced number
of cores, make sure you specify this in your `capabilities` section! **Do not run single threaded
processes on the distributed computing network - they will not go faster**.

Consider the following configuration:

```
meta:
  batch_name: example_build_batch_01
  namespace: "brooks-openfoam"
  capabilities: [python3, matplotlib]
  matrix: "@your-username:matrix.org"

python:
  initialize: 
    build_file: build.py

    required_files:
      - path: dataset_always.csv

  # a list of python files and their associated file to be included
  jobs:
    - name: job_1
      file: run.py
      required_files:
        - path: dataset_sometimes.csv
          alias: dataset.csv

    - name: job_2
      file: run.py
      required_files: []
```

Using the following as `run.py`:

```
import os
import random
import pandas
import matplotlib.pyplot as plt

# helper function to debug files
def print_files():
	# read the ./input directory
    input_files = list(os.listdir("input/"))

	# read the ./iniital_files directory
    initial_files = list(os.listdir("initial_files/"))

    print("input files:")
    print(input_files)

    print("initial files:")
    print(initial_files)

def plot_csv(csv_name):
	# if `csv_name` exists, read the data. Otherwise, generate random data
    if os.path.exists(csv_name):
        df = pandas.read_csv(csv_name)
        x = list(df["x"])
        y = list(df["y"])
        title = "perscribed_results"
    else:
        x = list(range(0,11))
        y = list(range(0,11))
        random.shuffle(y)
        title = "random_results"

	# plot the data on the global matplotlib plot
    plt.plot(x,y)
    plt.title(title)

    return title

def main():
	# `dataset_always` will always be present in `./input`
    _ = plot_csv("input/dataset_always.csv")

	# `./input/dataset.csv` will only *somtimes* exist, depending
	# on if we are running `job_1` or `job_2`
    save_name = plot_csv("input/dataset.csv")

	# export the figure to ./distribute_save so that it is archived on the server
    plt.savefig("distribute_save/" + save_name + ".png", bbox_inches="tight")

if __name__ == "__main__":
    main()
```

## TODO: include generated figures here
