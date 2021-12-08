import os
import random
import pandas
import matplotlib.pyplot as plt
import sys

# helper function to debug files
def print_files():
    input_files = list(os.listdir("input/"))
    initial_files = list(os.listdir("initial_files/"))

    print("input files:")
    print(input_files)
    print("initial files:")
    print(initial_files)

def plot_csv(csv_name):
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

    plt.plot(x,y)
    plt.title(title)

    return title

def main():
    _ = plot_csv("/input/dataset_always.csv")
    save_name = plot_csv("/input/dataset.csv")


    plt.savefig("/distribute_save/" + save_name + ".png", bbox_inches="tight")
if __name__ == "__main__":
    print(sys.argv);
    main()
