FROM rust:bullseye
RUN apt-get update && \
    apt-get install ca-certificates curl gnupg lsb-release jq git -y && \mkdir -m 0755 -p /etc/apt/keyrings && \
    curl -fsSL https://download.docker.com/linux/debian/gpg | gpg --dearmor -o /etc/apt/keyrings/docker.gpg && \
    echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian $(lsb_release -cs) stable" | tee /etc/apt/sources.list.d/docker.list > /dev/null && \
    apt-get update && \
    apt-get install docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin -y && \
    apt-get clean && \
    echo '%sudo ALL=(ALL) NOPASSWD:ALL' >> /etc/sudoers

RUN cargo install cargo-deb mdbook
RUN rustup component add rustfmt

WORKDIR /src
CMD ["bash", "-c", "rm /var/run/docker.pid; dockerd"]