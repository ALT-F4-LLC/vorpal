FROM docker.io/library/debian:sid-slim@sha256:7bee69f270ab968dbb059bd9b33100503b9c4f52e8d0da2c267d48e4b06bac3d

RUN apt-get update \
    && apt-get install --no-install-recommends --yes \
    bison \
    build-essential \
    ca-certificates \
    curl \
    file \
    gawk \
    python3 \
    texinfo \
    && rm -rf /var/lib/apt/lists/*
