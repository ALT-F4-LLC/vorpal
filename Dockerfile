FROM docker.io/library/rust:1.80.0@sha256:fcbb950e8fa0de7f8ada015ea78e97ad09fcc4120bf23485664e418e0ec5087b AS build

RUN apt-get update \
    && apt-get install -y \
    openssl \
    protobuf-compiler \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/app

RUN mkdir -p cli/src \
    && mkdir -p notary/src \
    && mkdir -p schema/src \
    && mkdir -p store/src \
    && mkdir -p worker/src \
    && touch cli/src/main.rs \
    && touch notary/src/lib.rs \
    && touch schema/src/lib.rs \
    && touch store/src/lib.rs \
    && touch worker/src/main.rs

COPY cli/Cargo.toml cli/Cargo.toml
COPY notary/Cargo.toml notary/Cargo.toml
COPY schema/Cargo.toml schema/Cargo.toml
COPY store/Cargo.toml store/Cargo.toml
COPY worker/Cargo.toml worker/Cargo.toml
COPY Cargo.toml Cargo.lock ./

RUN cargo vendor --versioned-dirs \
    && echo "[source.crates-io]" >> Cargo.toml \
    && echo "replace-with = 'vendored-sources'" >> Cargo.toml \
    && echo "[source.vendored-sources]" >> Cargo.toml \
    && echo "directory = '/usr/src/app/vendor'" >> Cargo.toml

COPY cli/src cli/src

COPY notary/src notary/src

COPY schema/api schema/api
COPY schema/src schema/src
COPY schema/build.rs schema/build.rs

COPY store/src store/src

COPY worker/src worker/src

RUN cargo check -j $(nproc) --offline --profile release
RUN cargo build -j $(nproc) --offline --profile release
RUN cargo test -j $(nproc) --offline --profile release -- --test-threads=$(nproc)


FROM docker.io/library/debian:12.6-slim@sha256:f528891ab1aa484bf7233dbcc84f3c806c3e427571d75510a9d74bb5ec535b33

RUN apt-get update && apt-get install -y \
    curl \
    libssl3 \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

COPY --from=build /usr/src/app/target/release/vorpal /usr/local/bin/vorpal
COPY --from=build /usr/src/app/target/release/vorpal-worker /usr/local/bin/vorpal-worker
