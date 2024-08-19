FROM docker.io/library/debian:12.6-slim@sha256:5f7d5664eae4a192c2d2d6cb67fc3f3c7891a8722cd2903cc35aa649a12b0c8d

RUN apt-get update && apt-get install -y \
    # autoconf \
    # automake \
    # binutils \
    # bison \
    # byacc \
    # coreutils \
    # dpkg-dev \
    # file \
    # g++ \
    # gawk \
    # help2man \
    # libc6-dev \
    # libssl-dev \
    # libtool \
    # m4 \
    # make \
    # perl \
    # rsync \
    # texinfo \
    ca-certificates \
    gcc \
    libssl-dev \
    pkg-config \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*
