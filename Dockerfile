FROM docker.io/library/debian:sid-slim@sha256:7bee69f270ab968dbb059bd9b33100503b9c4f52e8d0da2c267d48e4b06bac3d

RUN apt-get update \
    && apt-get install --yes \
    bash \
    binutils \
    bison \
    bubblewrap \
    bzip2 \
    ca-certificates \
    coreutils \
    curl \
    diffutils \
    g++ \
    gawk \
    gcc \
    grep \
    gzip \
    linux-headers-$(uname -r) \
    m4 \
    make \
    patch \
    perl \
    python3 \
    rsync \
    sed \
    tar \
    texinfo \
    xz-utils \
    zstd \
    && rm -rf /var/lib/apt/lists/*

RUN ln -sf /bin/bash /bin/sh \
    && [ ! -e /etc/bash.bashrc ] || mv -v /etc/bash.bashrc /etc/bash.bashrc.NOUSE \
    && groupadd --gid 1000 vorpal \
    && useradd -s /bin/bash -g vorpal -u 1000 -m -k /dev/null vorpal

USER vorpal

WORKDIR /home/vorpal

COPY --chmod=755 --chown=vorpal:vorpal script/version_check.sh version_check.sh

RUN ./version_check.sh
