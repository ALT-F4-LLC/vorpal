FROM docker.io/library/rust:1.79.0@sha256:9b2689d6f99ff381f178fa4361db745c8c355faecde73aa5b18b0efa84f03e62 AS build

RUN apt-get update \
    && apt-get install -y \
    openssl \
    protobuf-compiler \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/app

COPY Cargo.toml Cargo.lock ./

RUN cargo vendor --versioned-dirs \
    && echo "[source.crates-io]" >> Cargo.toml \
    && echo "replace-with = 'vendored-sources'" >> Cargo.toml \
    && echo "[source.vendored-sources]" >> Cargo.toml \
    && echo "directory = '/usr/src/app/vendor'" >> Cargo.toml

COPY api api
COPY src src
COPY build.rs build.rs

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

ENTRYPOINT ["vorpal"]
