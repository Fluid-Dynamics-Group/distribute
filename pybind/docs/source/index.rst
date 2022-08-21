.. distribute_compute_config documentation master file, created by
   sphinx-quickstart on Sat Aug 20 14:17:24 2022.
   You can adapt this file completely to your liking, but it should at least
   contain the root `toctree` directive.

Welcome to distribute_compute_config's documentation!
=====================================================

.. _installation:

Installation
------------

Install with ``pip``:

.. code-block:: console

   $ pip3 install distribute_compute_config 

Usage
--------

.. autofunction:: distribute_compute_config.apptainer_config(meta: Meta, description: Description) -> ApptainerConfig

.. autofunction:: distribute_compute_config.metadata(namespace: str, batch_name: str, capabilities: List[str], matrix_username=None) -> Meta

.. autofunction:: distribute_compute_config.description(initialize: Initialize, jobs: List[Job]) -> Description

.. autofunction:: distribute_compute_config.file(path: str, relative=False, alias=None) -> File

.. autofunction:: distribute_compute_config.initialize(sif_path: str, required_files: List[File], required_mounts: List[str]) -> Initialize

.. autofunction:: distribute_compute_config.job(name: str, required_files: List[File]) -> Job

.. autofunction:: distribute_compute_config.write_config_to_file(config: ApptainerConfig, path: str)

