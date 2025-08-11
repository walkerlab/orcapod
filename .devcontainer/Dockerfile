# DevContainer image
FROM ubuntu:24.04
RUN \
    # install docker
    apt-get update && \
    apt-get install ca-certificates curl -y && \
    mkdir -m 0755 -p /etc/apt/keyrings && \
    curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc && \
    chmod a+r /etc/apt/keyrings/docker.asc && \
    echo \
        "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu \
        $(. /etc/os-release && echo "${UBUNTU_CODENAME:-$VERSION_CODENAME}") stable" | \
        tee /etc/apt/sources.list.d/docker.list > /dev/null && \
    apt-get update && \
    apt-get install docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin -y && \
    usermod -aG docker ubuntu && \
    apt-get clean
RUN \
    # dev setup
    apt update && \
    apt-get install sudo jq bash-completion graphviz rsync software-properties-common gcc -y && \
    add-apt-repository ppa:git-core/ppa && \
    apt update && \
    apt-get install git -y && \
    usermod -aG sudo ubuntu && \
    echo '%sudo ALL=(ALL) NOPASSWD:ALL' >> /etc/sudoers && \
    echo '. /etc/bash_completion' >> /home/ubuntu/.bashrc && \
    echo 'export PS1="\[\e[32;1m\]\u\[\e[m\]@\[\e[34;1m\]\H\[\e[m\]:\[\e[33;1m\]\w\[\e[m\]$ "' >> /home/ubuntu/.bashrc && \
    chown ubuntu:nogroup /home/ubuntu/.bashrc && \
    apt-get clean

USER ubuntu
ENV CARGO_HOME=/home/ubuntu/.cargo \
    PATH=/home/ubuntu/.cargo/bin:/home/ubuntu/.local/bin:$PATH
RUN \
    # install rust
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain=1.87.0 -y && \
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
    cargo install cargo-expand
RUN \
    # install python manager
    curl -LsSf https://astral.sh/uv/install.sh | sh && \
    uv venv -p 3.10 ~/.local/share/base && \
    # pip package based on C lib/client
    uv pip install cffi maturin[patchelf] -p ~/.local/share/base && \
    # useful in examples
    uv pip install ipykernel eclipse-zenoh -p ~/.local/share/base && \
    echo '. ~/.local/share/base/bin/activate' >> ~/.bashrc
ENV VIRTUAL_ENV=/home/ubuntu/.local/share/base
CMD ["bash", "-c", "sudo rm /var/run/docker.pid; sudo dockerd"]
