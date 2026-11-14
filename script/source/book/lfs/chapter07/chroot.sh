# Linux From Scratch 12.4-systemd: chapter07/chroot
# https://www.linuxfromscratch.org/lfs/view/12.4-systemd/chapter07/chroot.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

chroot "$LFS" /usr/bin/env -i   \
    HOME=/root                  \
    TERM="$TERM"                \
    PS1='(lfs chroot) \u:\w\$ ' \
    PATH=/usr/bin:/usr/sbin     \
    MAKEFLAGS="-j$(nproc)"      \
    TESTSUITEFLAGS="-j$(nproc)" \
    /bin/bash --login
