# Linux From Scratch 12.4-systemd: chapter08/openssl
# https://www.linuxfromscratch.org/lfs/view/12.4-systemd/chapter08/openssl.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

./config --prefix=/usr         \
         --openssldir=/etc/ssl \
         --libdir=lib          \
         shared                \
         zlib-dynamic

make

HARNESS_JOBS=$(nproc) make test

sed -i '/INSTALL_LIBS/s/libcrypto.a libssl.a//' Makefile
make MANSUFFIX=ssl install

mv -v /usr/share/doc/openssl /usr/share/doc/openssl-3.5.2

cp -vfr doc/* /usr/share/doc/openssl-3.5.2
