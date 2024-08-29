FROM docker.io/library/rust:1.80.0@sha256:fcbb950e8fa0de7f8ada015ea78e97ad09fcc4120bf23485664e418e0ec5087b AS build

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
    && cat .cargo/config.toml

COPY cli cli
COPY notary notary
COPY schema schema
COPY store store
COPY worker worker

RUN cargo check -j $(nproc) --offline --release
RUN cargo build -j $(nproc) --offline --release
RUN cargo test -j $(nproc) --offline --release


FROM docker.io/library/debian:12.6-slim@sha256:5f7d5664eae4a192c2d2d6cb67fc3f3c7891a8722cd2903cc35aa649a12b0c8d

RUN apt-get update \
    && apt-get install --no-install-recommends --yes ca-certificates curl libssl-dev \
    && NICKEL_ARCH=$(uname -m) \
    && if [ "$NICKEL_ARCH" = "aarch64" ]; then \
    NICKEL_ARCH="arm64"; \
    fi \
    && curl -fsSL "https://github.com/tweag/nickel/releases/download/1.7.0/nickel-${NICKEL_ARCH}-linux" -o /usr/local/bin/nickel \
    && chmod +x /usr/local/bin/nickel \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

COPY --from=build /usr/src/app/target/release/vorpal /usr/local/bin/vorpal

ENTRYPOINT ["vorpal"]
