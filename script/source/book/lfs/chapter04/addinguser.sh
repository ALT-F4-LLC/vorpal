# Linux From Scratch 12.4-systemd: chapter04/addinguser
# https://www.linuxfromscratch.org/lfs/view/12.4-systemd/chapter04/addinguser.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

groupadd lfs
useradd -s /bin/bash -g lfs -m -k /dev/null lfs

passwd lfs

chown -v lfs $LFS/{usr{,/*},var,etc,tools}
case $(uname -m) in
  x86_64) chown -v lfs $LFS/lib64 ;;
esac

su - lfs
