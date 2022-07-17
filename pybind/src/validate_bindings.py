import distribute_config

def full_example():
    matrix_user = "@karik:matrix.org"
    batch_name = "test_batch"
    namespace = "test_namespace"
    capabilities = ["apptainer"]

    meta = distribute_config.metadata(namespace, batch_name, capabilities, matrix_user)

    sif_path = "./path/to/some/container.sif"
    # the contents of file.h5 will appear in the /input directory under the name `initial_condition.h5`
    initial_condition = distribute_config.file("./path/to/some/file.h5", relative=True, alias="initial_condition.h5")
    # a list of files that will /always/ be present in the /input directory of the container
    required_files = [initial_condition]
    # a list of paths inside the container that should be mounted to a folder on the host system.
    required_mounts = ["/solver/extra_mount"]

    print(type(required_files), type(required_files[0]))

    #initialize = distribute_config.initialize(sif_path, required_files, required_mounts)

    #
    # then put together two jobs that will be run using the container 
    #

    # config files will always appear in `/input` under the name `config.json`.
    # we use relative paths since otherwise the file must exist (and this is an example)
    job_1_config_file = distribute_config.file("./path/to/config1.json", alias="config.json", relative=True)
    job_2_config_file = distribute_config.file("./path/to/config2.json", alias="config.json", relative=True)

    # all `required files` will appear in the /input directory for their respective job
    job_1_required_files = [job_1_config_file]
    job_2_required_files = [job_2_config_file]

    job_1 = distribute_config.job("job_1", job_1_required_files)
    job_2 = distribute_config.job("job_2", job_2_required_files)

    jobs = [job_1, job_2]


    #
    # then put together a full description of the jobs that we will run
    # and the container we will use to run then
    #

    description = distribute_config.description(initialize, jobs)

    jobset = distribute_config.ApptainerJobset(meta, description)
    #jobset.write_to_path("./distribute-jobs.yaml")


def foo_debug():
    foo = distribute_config.make_foo()
    foo_wrapper  = distribute_config.make_foo_wrapper()

    distribute_config.take_foo(foo)
    distribute_config.take_foo_wrapper(foo_wrapper)

    print(foo, foo_wrapper)

#foo_debug()
full_example()

print("done")

