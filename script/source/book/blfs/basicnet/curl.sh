# Beyond Linux From Scratch 12.4-systemd: basicnet/curl
# https://www.linuxfromscratch.org/blfs/view/12.4-systemd/basicnet/curl.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

./configure --prefix=/usr    \
            --disable-static \
            --with-openssl   \
            --with-ca-path=/etc/ssl/certs &&
make

# (as root)
make install &&

rm -rf docs/examples/.deps &&

find docs \( -name Makefile\* -o  \
             -name \*.1       -o  \
             -name \*.3       -o  \
             -name CMakeLists.txt \) -delete &&

cp -v -R docs -T /usr/share/doc/curl-8.15.0
