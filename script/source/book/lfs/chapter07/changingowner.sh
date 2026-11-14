# Linux From Scratch 12.4-systemd: chapter07/changingowner
# https://www.linuxfromscratch.org/lfs/view/12.4-systemd/chapter07/changingowner.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

chown --from lfs -R root:root $LFS/{usr,var,etc,tools}
case $(uname -m) in
  x86_64) chown --from lfs -R root:root $LFS/lib64 ;;
esac
