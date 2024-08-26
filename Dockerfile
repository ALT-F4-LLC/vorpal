FROM docker.io/library/rust:1.80.0-slim@sha256:fcbb950e8fa0de7f8ada015ea78e97ad09fcc4120bf23485664e418e0ec5087b AS dev

RUN ARCH=$(uname -m) && \
    if [ "$ARCH" = "aarch64" ]; then \
    ARCH="arm64"; \
    fi && \
    echo "Architecture set to $ARCH" && \
    apt-get update \
    && apt-get install --no-install-recommends --yes curl \
    && install -m 0755 -d /etc/apt/keyrings \
    && curl -fsSL https://download.docker.com/linux/debian/gpg -o /etc/apt/keyrings/docker.asc \
    && chmod a+r /etc/apt/keyrings/docker.asc \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/debian $(. /etc/os-release && echo "$VERSION_CODENAME") stable" | tee /etc/apt/sources.list.d/docker.list > /dev/null \
    && apt-get update \
    && apt-get install --no-install-recommends --yes \
    docker-buildx-plugin \
    docker-ce-cli \
    docker-compose-plugin \
    libssl-dev \
    pkg-config \
    protobuf-compiler \
    && curl -fsSL https://github.com/tweag/nickel/releases/download/1.7.0/nickel-$ARCH-linux -o /usr/local/bin/nickel \
    && chmod +x /usr/local/bin/nickel \
    && curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh | bash -s -- --to /usr/local/bin \
    && rustup component add clippy rust-analyzer rust-src rustfmt \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

RUN echo $PATH

FROM dev AS build

WORKDIR /usr/src/app

RUN mkdir -p cli/src notary/src schema/src store/src worker/src && \
    touch cli/src/main.rs notary/src/lib.rs schema/src/lib.rs store/src/lib.rs worker/src/lib.rs

COPY Cargo.toml Cargo.lock ./
COPY cli/Cargo.toml cli/Cargo.toml
COPY notary/Cargo.toml notary/Cargo.toml
COPY schema/Cargo.toml schema/Cargo.toml
COPY store/Cargo.toml store/Cargo.toml
COPY worker/Cargo.toml worker/Cargo.toml

RUN cargo vendor --versioned-dirs \
    && echo "[source.crates-io]" >> Cargo.toml \
    && echo "replace-with = 'vendored-sources'" >> Cargo.toml \
    && echo "[source.vendored-sources]" >> Cargo.toml \
    && echo "directory = '/usr/src/app/vendor'" >> Cargo.toml

COPY cli cli
COPY notary notary
COPY schema schema
COPY store store
COPY worker worker

RUN cargo build -j $(nproc) --offline --profile release


FROM docker.io/library/debian:12.6-slim@sha256:5f7d5664eae4a192c2d2d6cb67fc3f3c7891a8722cd2903cc35aa649a12b0c8d

RUN apt-get update && apt-get install -y \
    curl \
    libssl3 \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

COPY --from=build /usr/src/app/target/release/vorpal /usr/local/bin/vorpal

ENTRYPOINT ["vorpal"]
