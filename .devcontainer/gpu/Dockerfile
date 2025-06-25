# DevContainer image
FROM nvidia/cuda:12.6.1-cudnn-runtime-ubuntu24.04

RUN \
    # install docker
    apt-get update && \
    apt-get install ca-certificates curl gnupg lsb-release -y && \
    mkdir -m 0755 -p /etc/apt/keyrings && \
    curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc && \
    echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu $(. /etc/os-release && echo "$VERSION_CODENAME") stable" | tee /etc/apt/sources.list.d/docker.list > /dev/null && \
    apt-get update && \
    apt-get install docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin -y && \
    usermod -aG docker root && \
    apt-get clean

RUN \
    # dev setup
    apt update && \
    apt-get install build-essential sudo jq git bash-completion graphviz rsync -y && \
    echo '%sudo ALL=(ALL) NOPASSWD:ALL' >> /etc/sudoers && \
    echo '. /etc/bash_completion' >> /root/.bashrc && \
    echo 'export PS1="\[\e[32;1m\]\u\[\e[m\]@\[\e[34;1m\]\H\[\e[m\]:\[\e[33;1m\]\w\[\e[m\]$ "' >> /root/.bashrc && \
    apt-get clean

ENV PATH="/root/.cargo/bin:${PATH}"
RUN apt-get update && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y && \
    # rust auto formatting
    rustup component add rustfmt && \
    # rust style linter
    rustup component add clippy && \
    # rust code coverage
    cargo install cargo-llvm-cov && \
    rustup component add llvm-tools-preview && \
    # rust crate structure diagram
    cargo install cargo-modules && \
    # expand rust macros (useful in debugging)
    cargo install cargo-expand && \
    apt-get clean

ENV PATH=${PATH}:/root/.local/bin
RUN \
    # install python manager
    curl -LsSf https://astral.sh/uv/install.sh | sh && \
    uv venv -p 3.10 ~/.local/share/base && \
    # pip package based on C lib/client
    uv pip install cffi maturin ipykernel -p ~/.local/share/base && \
    echo '. ~/.local/share/base/bin/activate' >> ~/.bashrc
ENV VIRTUAL_ENV=/root/.local/share/base
CMD ["bash", "-c", "sudo rm /var/run/docker.pid; sudo dockerd"]
