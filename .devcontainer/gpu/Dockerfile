# DevContainer image
FROM nvidia/cuda:12.6.1-cudnn-runtime-ubuntu24.04

RUN \
    adduser --system --disabled-password --shell /bin/bash --home /home/vscode vscode && \
    # install docker
    apt-get update && \
    apt-get install ca-certificates curl gnupg lsb-release -y && \
    mkdir -m 0755 -p /etc/apt/keyrings && \
    curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc && \
    echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu $(. /etc/os-release && echo "$VERSION_CODENAME") stable" | tee /etc/apt/sources.list.d/docker.list > /dev/null && \
    apt-get update && \
    apt-get install docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin -y && \
    usermod -aG docker vscode && \
    apt-get clean

RUN apt-get update && curl https://sh.rustup.rs -sSf | bash -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

RUN \
    # dev setup
    apt update && \
    apt-get install sudo jq git bash-completion -y && \
    usermod -aG sudo vscode && \
    echo '%sudo ALL=(ALL) NOPASSWD:ALL' >> /etc/sudoers && \
    echo '. /etc/bash_completion' >> /home/vscode/.bashrc && \
    echo 'export PS1="\[\e[32;1m\]\u\[\e[m\]@\[\e[34;1m\]\H\[\e[m\]:\[\e[33;1m\]\w\[\e[m\]$ "' >> /home/vscode/.bashrc && \
    # rust auto styling
    rustup component add rustfmt && \
    apt-get clean

USER vscode
ENV PATH=${PATH}:/home/vscode/.local/bi
CMD ["bash", "-c", "sudo rm /var/run/docker.pid; sudo dockerd"]