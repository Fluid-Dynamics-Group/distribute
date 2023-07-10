# print all files in /input
import os 
print("/input directory files are:")
print(list(os.listdir("/input")))

# read the input file supplied from distribute
with open("/input/input.txt", "r") as f:
    text = f.read()
    number = int(text)

# square the number
square = number * number

# write the result to the output
with open("/distribute_save/output.txt", "w") as f:
    f.write(str(square))
