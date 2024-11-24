FROM docker.io/library/debian:sid-slim@sha256:377cd68c1412de57c47aa95c1535caf04982dd67431983e19fa68abfff9504c5

RUN ARCH=$(uname -m) \
    && if [ "${ARCH}" = "aarch64" ]; then ARCH="arm64"; fi \
    && if [ "${ARCH}" = "x86_64" ]; then ARCH="amd64"; fi \
    && echo "Current architecture: ${ARCH}" \
    && apt-get update \
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
    linux-headers-$ARCH \
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
