# Linux From Scratch 12.4-systemd: chapter08/cleanup
# https://www.linuxfromscratch.org/lfs/view/12.4-systemd/chapter08/cleanup.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

rm -rf /tmp/{*,.*}

find /usr/lib /usr/libexec -name \*.la -delete

find /usr -depth -name $(uname -m)-lfs-linux-gnu\* | xargs rm -rf

userdel -r tester
