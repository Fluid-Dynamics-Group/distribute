API Reference
=============

The recommended way to use the API is to start by looking at the final function :func:`distribute_compute_config.write_config_to_file` and work backwards 
to all the constituent functions.

.. autofunction:: distribute_compute_config.apptainer_config(meta: Meta, description: Description, slurm: Optional[Slurm] = None) -> ApptainerConfig

.. autofunction:: distribute_compute_config.metadata(namespace: str, batch_name: str, capabilities: List[str], matrix_username:Optional[str]=None) -> Meta

.. autofunction:: distribute_compute_config.description(initialize: Initialize, jobs: List[Job]) -> Description

.. autofunction:: distribute_compute_config.file(path: str, relative=False, alias=None) -> File

.. autofunction:: distribute_compute_config.initialize(sif_path: str, required_files: List[File], required_mounts: List[str]) -> Initialize

.. autofunction:: distribute_compute_config.job(name: str, required_files: List[File], slurm: Optional[Slurm] = None) -> Job

.. autofunction:: distribute_compute_config.write_config_to_file(config: ApptainerConfig, path: str)

.. autofunction:: distribute_compute_config.slurm(job_name: Optional[str] = None, output: Optional[str] = None, nodes: Optional[int] = None, ntasks: Optional[int] = None, cpus_per_task: Optional[int] = None, mem_per_cpu: Optional[str] = None, hint: Optional[str] = None, time: Optional[str] = None, partition: Optional[str] = None, account: Optional[str] = None, mail_user: Optional[str] = None, mail_type: Optional[str] = None) -> Slurm
