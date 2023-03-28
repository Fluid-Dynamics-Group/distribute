# Storing Results

Archiving jobs to the head node is easy. Ensure that your execution script moves all files
you wish to save to the `./distribute_save` folder before exiting. `distribute` will automatically read all the files
in `./distribute_save` and save them to the corresponding job folder on the head node permenantly. `distribute` will 
also clear out the `./distribute_save` folder for you between jobs so that you dont end up with duplicate files.

However, since different jobs from different batches may be scheduled out of order, **you should not rely on any files being 
present in the working directories from a previous job in your batch**. However, if your scripts *do* leave files
in the working directories, `distribute` does not actively monitor them and remove them between jobs.

In general, it is best to keep the execution of your scripts stateless: remove all temporary files
and created directories such that the next job scheduled on a given compute machine does not encounter
unexpected folders / files.
