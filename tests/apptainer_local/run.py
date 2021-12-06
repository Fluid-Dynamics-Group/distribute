import sys

def main():
    procs = int(sys.argv[1])
    print(f"running with {procs} processors")

    print("writing to /dir1")
    with open("/dir1/file1.txt", "w") as f:
        f.write("checking mutability of file system")

    print("writing to /dir2")
    with open("/dir2/file2.txt", "w") as f:
        f.write("checking mutability of file system")

    # read some input files from /input

    print("reading input files")
    with open("/input/input.txt", "r") as f:
        text = f.read()
        num = int(text)

    with open("/distribute_save/simulated_output.txt", "w") as f:
        square = num * num
        f.write(f"the square of the input was {square}")

if __name__ == "__main__":
    main()
