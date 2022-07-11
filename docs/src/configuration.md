# Configuration

Configuration files are fundamental to how `distribute` works. Without a configuration file, the server would not 
know what nodes that jobs could be run on, or even what the content of each job is. Configuration files 
are also useful in `pull`ing the files you want from your compute job to your local machine. Therefore,
they are imperative to understand.

## Configuration files

As mentioned in the introduction, configuration files (usually named `distribute-jobs.yaml`) come in two flavors:
python scripts and apptainer images. 

The advantage of python scripts is that they are relatively easy to produce:
you need to have a single script that specifies how to build your project, and another script (for each job) that specifies
how to run each file. The disadvantage of python configurations is that they are very brittle - the exact server configuration may
be slightly different from your environment and therefore can fail in unpredictable ways. 
Since all nodes with your capabilities are treated equally, a node failing to execute 
your files will quickly chew through your jobs and spit out some errors.

The advantage of apptainer jobs is that you can be sure that **the way the job is run 
on `distribute` nodes is exactly how it would run on your local machine**. This means that, while it may take 
slightly longer to make a apptainer job, you can directly ensure that all the dependencies are present, and that there wont
be any unexpected differences in the environment to ruin your job execution. *The importance of this cannot be
understated*. The other advantage of apptainer jobs is that they can be directly run on other compute clusters (as
well as every lab machine), and they are much easier to debug if you want to hand off the project to another lab 
member for help. The disadvantage of apptainer jobs is that *the file system is not mutable* - you cannot write 
to any files in the the container. Any attempt to write a file in the apptainer filesystem will result in an error 
and the job will fail. Fear not, the fix for this is relatively easy: you will just bind folders from the host file system 
(via configuration file) to your container that *will* be writeable. All you have to do then is ensure that your
compute job only writes to folders that have been bound to the container from the host filesystem.

Regardless of using a python or apptainer configuration, the three main areas of the configuration file remain the same:

<table>
  <tr>
    <th>Section</th>
    <th>Python Configuration</th>
    <th>Apptainer Configuration</th>
  </tr>
  <tr>
    <td>Meta</td>
    <td>
		<ul>
			<li> 
			Specifies how the files are saved on the head node (<code class="hljs">namespace</code> and <code class="hljs">batch_name</code> fields)
			</li>
			<li>
				Describes all the
				"<code class="hljs">capabilities</code>"
				that are required to actually run the file. Nodes that do not meet your 
				<code class="hljs">capabilities</code> will not have the job scheduled on them.
			</li>
			<li>
				Provides an optional field for your matrix username. If specified, you will receive 
				a message on matrix when all your jobs are completed.
			</li>
		</ul>
	</td>
    <td>
		The same as python
	</td>
  </tr>
  <tr>
    <td>
		Building
	</td>
    <td>
		<ul>
			<li>specifies a path to a python file </li>
			<ul>
				<li>Clone all repositories you require</li>
				<li>Compile your project and make sure everything is ready for jobs</li>
			</ul>
			<li>Gives the paths to some files you want to be available on the node when you are compiling</li>
		</ul>
	</td>
    <td>
		<ul>
			<li> Gives the path to a apptainer image file (compiled on your machine)</li>
		</ul>
	</td>
  </tr>
  <tr>
    <td>
		Running 
	</td>
    <td>
		<ul>
			<li>
			A list of jobs names
				<ul>
					<li>
					Each job specifies a python file and some additional files you want to be present
					</li>
					<li>
					Your python file will drop you in the exact same directory that you built from. You 
					are responsible for finding and running your previously compiled project with (optionally)
					whatever input files you have ensured are present ( in ./input).
					</li>
				</ul>
			</li>
		</ul>
	</td>
    <td>
		<ul>
			<li>
				A list of job names
				<ul>
					<li> 
					Similarly, also specify the files you want to be present
					</li>
					<li> 
					the /input directory of your container will contain all the files you specify in each job section
					</li>
					<li> 
					You are responsible for reading in the input files and running the solver
					</li>
				</ul>
			</li>
			<li> 
			You dont need to specify any run time scripts
			</li>
		</ul>
	</td>
  </tr>
</table>

## How files are saved

Files are saved on the server using your `namespace`, `batch_name`, and `job_name`s. Take the following configuration file that specifies
a apptainer job that does not save any of its own files:

```yaml
meta:
  batch_name: example_jobset_name
  namespace: example_namespace
  matrix: @your-username:matrix.org
  capabilities: []
apptainer:
  initialize:
    sif: execute_container.sif
    required_files: []
    required_mounts:
      - /path/inside/container/to/mount
  jobs:
    - name: job_1
      required_files: []
    - name: job_2
      required_files: []
    - name: job_3
      required_files: []
```

The resulting folder structure on the head node will be

```
.
└── example_namespace
    └── example_jobset_name
        ├── example_jobset_name_build_ouput-node-1.txt
        ├── example_jobset_name_build_ouput-node-2.txt
        ├── example_jobset_name_build_ouput-node-3.txt
        ├── job_1
        │   └── stdout.txt
        ├── job_2
        │   └── stdout.txt
        └── job_3
            └── stdout.txt
```

The nice thing about `distribute` is that you also receive the output that would appear on your terminal 
as a text file. Namely, you will have text files for how your project was compiled (`example_jobset_name_build_ouput-node-1.txt` 
is the python build script output for node-1), as well as the output for each job inside each respective folder.

If you were to execute another configuration file using a different batch name, like this:

```yaml
meta:
  batch_name: example_jobset_name
  namespace: example_namespace
  matrix: @your-username:matrix.org
  capabilities: []

# -- snip -- #
```

the output would look like this:

```
.
└── example_namespace
    ├── another_jobset
    │   ├── example_jobset_name_build_ouput-node-1.txt
    │   ├── example_jobset_name_build_ouput-node-2.txt
    │   ├── example_jobset_name_build_ouput-node-3.txt
    │   ├── job_1
    │   │   └── stdout.txt
    │   ├── job_2
    │   │   └── stdout.txt
    │   └── job_3
    │       └── stdout.txt
    └── example_jobset_name
        ├── example_jobset_name_build_ouput-node-1.txt
        ├── example_jobset_name_build_ouput-node-2.txt
        ├── example_jobset_name_build_ouput-node-3.txt
        ├── job_1
        │   └── stdout.txt
        ├── job_2
        │   └── stdout.txt
        └── job_3
            └── stdout.txt
```

Therefore, its important to **ensure that your `batch_name` fields are unique**. If you don't, the output of
the previous batch will be deleted or combined with the new job.

## Examples

Examples creating each configuration file can be found in the current page's subchapters.
