FROM docker.io/library/rust:1.80.0@sha256:fcbb950e8fa0de7f8ada015ea78e97ad09fcc4120bf23485664e418e0ec5087b

RUN apt-get update \
    && apt-get install --yes \
    openssl \
    protobuf-compiler \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/app

RUN mkdir -p cli/src \
    notary/src \
    schema/src \
    store/src \
    worker/src \
    && touch cli/src/main.rs \
    notary/src/lib.rs \
    schema/src/lib.rs \
    store/src/lib.rs \
    worker/src/lib.rs

COPY cli/Cargo.toml cli/Cargo.toml
COPY notary/Cargo.toml notary/Cargo.toml
COPY schema/Cargo.toml schema/Cargo.toml
COPY store/Cargo.toml store/Cargo.toml
COPY worker/Cargo.toml worker/Cargo.toml
COPY Cargo.lock Cargo.toml ./

RUN mkdir -p .cargo/vendor \
    && CARGO_VENDOR=$(cargo vendor --versioned-dirs .cargo/vendor) \
    echo "${CARGO_VENDOR}" > .cargo/config.toml \
    && cat .cargo/config.toml \
    && ls -alh .cargo/vendor

COPY cli cli
COPY notary notary
COPY schema schema
COPY store store
COPY worker worker

RUN cargo check -j $(nproc) --offline --release

RUN cargo build -j $(nproc) --offline --release

RUN cargo test -j $(nproc) --offline --release
