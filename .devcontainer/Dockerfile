# DevContainer image
FROM rust:1.81-slim

RUN \
    # install docker
    apt-get update && \
    apt-get install ca-certificates curl gnupg lsb-release -y && \
    mkdir -m 0755 -p /etc/apt/keyrings && \
    curl -fsSL https://download.docker.com/linux/debian/gpg | gpg --dearmor -o /etc/apt/keyrings/docker.gpg && \
    echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian $(lsb_release -cs) stable" | tee /etc/apt/sources.list.d/docker.list > /dev/null && \
    apt-get update && \
    apt-get install docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin -y && \
    usermod -aG docker root && \
    apt-get clean

RUN \
    # dev setup
    apt update && \
    apt-get install sudo jq git bash-completion -y && \
    # rust auto styling
    rustup component add rustfmt && \
    apt-get clean

ENV PATH=${PATH}:/home/vscode/.local/bin

CMD ["bash", "-c", "sudo rm /var/run/docker.pid; sudo dockerd"]