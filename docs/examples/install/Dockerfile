FROM python:slim

## install Rust + Git
RUN \
    apt update && \
    apt install curl gcc git -y && \
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y

## install remote
RUN \
    . "$HOME/.cargo/env" && \
    pip install git+https://github.com/guzman-raphael/orcapod@uniffi-client

## install locally
# COPY . /tmp/orcapod
# RUN \
#     . "$HOME/.cargo/env" && \
#     pip install /tmp/orcapod && \
#     rm -R /tmp/orcapod
