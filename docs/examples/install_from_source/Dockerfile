FROM python:slim
SHELL ["bash", "-lc"]

## install Rust + Git
RUN \
    apt update && \
    apt install curl gcc git -y && \
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y && \
    echo ". ~/.cargo/env" >> ~/.bashrc

## install remote
RUN \
    pip install git+https://github.com/guzman-raphael/orcapod@uniffi-client

## install locally
# COPY . /tmp/orcapod
# RUN \
#     pip install /tmp/orcapod && \
#     rm -R /tmp/orcapod

WORKDIR /root
