# Machines

| name       | physical core count | address        | role      |
|------------|---------------------|----------------|-----------|
| lab1       | 16                  | 134.197.94.134 | compute   |
| lab2       | 16                  | 134.197.27.105 | compute   |
| lab3       | 32                  | 134.197.94.104 | compute   |
| lab4       | 12                  | 134.197.94.155 | storage   |
| lab5       | 32                  | 134.197.27.180 | compute   |
| distserver | 12                  | 134.197.95.21  | head node |

* If you are adding a job with `distribute add`, you should use `distserver` ip address.
* If you are updating the `distribute` source code on all machines, you should use `lab1`, `lab2`, `lab3` and `distserver`
	* `lab4` server must be accessed through a FreeBSD jail
