# Build Scripts

The build script is specified in the `initialize` section under the `build_file` and `path` keys. 
The build script is simply responsible for cloning relevant git repositories and compiling any scripts in the project. 

Since your build scripts will be run as an unpriviledged user without any credentials setup, it is not generally
possible to clone private github repositories from the build file. If you require access to these repositories,
consider using a `apptainer` based configuration.

An example build script to compile a project might look like this:


```
import subprocess
import os

HIT3D = "https://github.com/Fluid-Dynamics-Group/hit3d.git"

# execute a command as if it was in a terminal
def run_shell_command(command):
    print(f"running {command}")
    output = subprocess.run(command,shell=True, check=True)

	# if there was some text output, print it to the terminal
	# so that distribute has access to it. Otherwise, the script
	# output is unknown
    if not output.stdout is None:
        print(output.stdout)

# clone a github repository
def make_clone_url(ssh_url):
    return f"git clone {ssh_url} --depth 1"

def main():
    # save the current directory that we are operating in
    root = os.getcwd()

    # clone the repository
    run_shell_command(make_clone_url(HIT3D))

    # go into the source directory and compile the project
    os.chdir("hit3d/src")
    run_shell_command("make")

    os.chdir(root)

if __name__ == "__main__":
    main()
```

After this script is executed, a binary will be placed at `./hit3d/src/hit3d.x` that may be used in future execution scripts.
