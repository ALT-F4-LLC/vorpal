# Linux From Scratch 12.4-systemd: chapter07/gettext
# https://www.linuxfromscratch.org/lfs/view/12.4-systemd/chapter07/gettext.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

./configure --disable-shared

make

cp -v gettext-tools/src/{msgfmt,msgmerge,xgettext} /usr/bin
