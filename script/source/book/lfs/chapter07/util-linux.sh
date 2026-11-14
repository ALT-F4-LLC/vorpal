# Linux From Scratch 12.4-systemd: chapter07/util-linux
# https://www.linuxfromscratch.org/lfs/view/12.4-systemd/chapter07/util-linux.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

mkdir -pv /var/lib/hwclock

./configure --libdir=/usr/lib     \
            --runstatedir=/run    \
            --disable-chfn-chsh   \
            --disable-login       \
            --disable-nologin     \
            --disable-su          \
            --disable-setpriv     \
            --disable-runuser     \
            --disable-pylibmount  \
            --disable-static      \
            --disable-liblastlog2 \
            --without-python      \
            ADJTIME_PATH=/var/lib/hwclock/adjtime \
            --docdir=/usr/share/doc/util-linux-2.41.1

make

make install
