import subprocess
import os

HIT3D = "https://github.com/Fluid-Dynamics-Group/hit3d.git"

def run_shell_command(command):
    print(f"running {command}")
    output = subprocess.run(command,shell=True, check=True)
    if not output.stdout is None:
        print(output.stdout)

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
