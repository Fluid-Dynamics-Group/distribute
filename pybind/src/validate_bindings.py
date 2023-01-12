import distribute_compute_config as distribute 

matrix_user = "@matrix_id:matrix.org"
batch_name = "test_batch"
namespace = "test_namespace"
capabilities = ["apptainer"]

meta = distribute.metadata(namespace, batch_name, capabilities, matrix_user)

sif_path = "./path/to/some/container.sif"
sif = distribute.file(sif_path, relative = True)

# the contents of file.h5 will appear in the /input directory under the name `initial_condition.h5`
initial_condition = distribute.file("./path/to/some/file.h5", relative=True, alias="initial_condition.h5")
# a list of files that will /always/ be present in the /input directory of the container
required_files = [initial_condition]
# a list of paths inside the container that should be mounted to a folder on the host system.
required_mounts = ["/solver/extra_mount"]

initialize = distribute.initialize(sif, required_files, required_mounts)

#
# then put together two jobs that will be run using the container 
#

# config files will always appear in `/input` under the name `config.json`.
# we use relative paths since otherwise the file must exist (and this is an example)
job_1_config_file = distribute.file("./path/to/config1.json", alias="config.json", relative=True)
job_2_config_file = distribute.file("./path/to/config2.json", alias="config.json", relative=True)

# all `required files` will appear in the /input directory for their respective job
job_1_required_files = [job_1_config_file]
job_2_required_files = [job_2_config_file]

job_1 = distribute.job("job_1", job_1_required_files)
job_2 = distribute.job("job_2", job_2_required_files)

jobs = [job_1, job_2]

#
# then put together a full description of the jobs that we will run
# and the container we will use to run then
#

description = distribute.description(initialize, jobs)


# set up some parameters so that we can run this job with slurm
slurm = distribute.slurm(
    output = "output.txt", 
    nodes = 1, 
    ntasks = 4, 
    cpus_per_task = 1, 
    # 10 megabytes of memory allocated
    mem_per_cpu = "10M",
    hint = "nomultithread",
    # 30 minutes of runtime
    time = "00:30:00",
    partition = "cpu-core-0",
    account = "my_account"
)

jobset = distribute.apptainer_config(meta, description, slurm = slurm)
distribute.write_config_to_file(jobset,"./distribute-jobs.yaml")
