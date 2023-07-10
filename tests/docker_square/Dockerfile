FROM ubuntu:22.04

RUN apt update -y && apt upgrade -y
RUN apt install python3 -y
RUN python3 --help

COPY ./main.py /main.py

ENTRYPOINT ["python3", "/main.py"]
