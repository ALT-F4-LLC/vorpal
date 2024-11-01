use vorpal_schema::vorpal::package::v0::PackageSandboxPath;

pub struct SandboxDefaultPaths {
    pub autoconf: bool,
    pub automake: bool,
    pub bash: bool,
    pub binutils: bool,
    pub bison: bool,
    pub bzip2: bool,
    pub curl: bool,
    pub coreutils: bool,
    pub diffutils: bool,
    pub file: bool,
    pub findutils: bool,
    pub flex: bool,
    pub gawk: bool,
    pub gcc: bool,
    pub gcc_12: bool,
    pub glibc: bool,
    pub grep: bool,
    pub gzip: bool,
    pub help2man: bool,
    pub includes: bool,
    pub lib: bool,
    pub m4: bool,
    pub make: bool,
    pub patchelf: bool,
    pub perl: bool,
    pub python: bool,
    pub sed: bool,
    pub tar: bool,
    pub texinfo: bool,
    pub wget: bool,
}

pub fn add_paths(options: SandboxDefaultPaths) -> Vec<PackageSandboxPath> {
    let mut paths = vec!["/etc/resolv.conf", "/etc/ssl/certs"];

    if options.autoconf {
        paths.extend([
            "/usr/bin/autoconf",
            "/usr/bin/autoheader",
            "/usr/bin/autom4te",
            "/usr/bin/autoreconf",
            "/usr/bin/autoscan",
            "/usr/bin/autoupdate",
            "/usr/bin/ifnames",
            "/usr/share/autoconf",
        ]);
    }

    if options.automake {
        paths.extend([
            "/usr/bin/aclocal",
            "/usr/bin/aclocal-1.16",
            "/usr/bin/automake",
            "/usr/bin/automake-1.16",
            "/usr/share/aclocal",
            "/usr/share/aclocal-1.16",
            "/usr/share/automake-1.16",
        ]);
    }

    if options.bash {
        paths.push("/bin/bash");
        paths.push("/bin/bashbug");
    }

    if options.binutils {
        paths.extend([
            "/usr/bin/addr2line",
            "/usr/bin/ar",
            "/usr/bin/as",
            "/usr/bin/c++filt",
            "/usr/bin/elfedit",
            "/usr/bin/gprof",
            "/usr/bin/ld",
            "/usr/bin/ld.bfd",
            "/usr/bin/nm",
            "/usr/bin/objcopy",
            "/usr/bin/objdump",
            "/usr/bin/ranlib",
            "/usr/bin/readelf",
            "/usr/bin/size",
            "/usr/bin/strings",
            "/usr/bin/strip",
        ]);
    }

    if options.bison {
        paths.extend(["/usr/bin/bison", "/usr/bin/yacc", "/usr/share/bison"]);
    }

    if options.bzip2 {
        paths.extend([
            "/usr/bin/bunzip2",
            "/usr/bin/bzcat",
            "/usr/bin/bzcmp",
            "/usr/bin/bzdiff",
            "/usr/bin/bzegrep",
            "/usr/bin/bzfgrep",
            "/usr/bin/bzgrep",
            "/usr/bin/bzip2",
            "/usr/bin/bzip2recover",
            "/usr/bin/bzless",
            "/usr/bin/bzmore",
        ]);
    }

    if options.curl {
        paths.extend(["/usr/bin/curl"]);
    }

    if options.coreutils {
        paths.extend([
            "/usr/bin/[",
            "/usr/bin/b2sum",
            "/usr/bin/base32",
            "/usr/bin/base64",
            "/usr/bin/basename",
            "/usr/bin/basenc",
            "/usr/bin/cat",
            "/usr/bin/chcon",
            "/usr/bin/chgrp",
            "/usr/bin/chmod",
            "/usr/bin/chown",
            "/usr/bin/cksum",
            "/usr/bin/comm",
            "/usr/bin/cp",
            "/usr/bin/csplit",
            "/usr/bin/cut",
            "/usr/bin/date",
            "/usr/bin/dd",
            "/usr/bin/df",
            "/usr/bin/dir",
            "/usr/bin/dircolors",
            "/usr/bin/dirname",
            "/usr/bin/du",
            "/usr/bin/echo",
            "/usr/bin/env",
            "/usr/bin/expand",
            "/usr/bin/expr",
            "/usr/bin/factor",
            "/usr/bin/false",
            "/usr/bin/fmt",
            "/usr/bin/fold",
            "/usr/bin/groups",
            "/usr/bin/head",
            "/usr/bin/hostid",
            "/usr/bin/id",
            "/usr/bin/install",
            "/usr/bin/join",
            "/usr/bin/kill",
            "/usr/bin/link",
            "/usr/bin/ln",
            "/usr/bin/logname",
            "/usr/bin/ls",
            "/usr/bin/md5sum",
            "/usr/bin/mkdir",
            "/usr/bin/mkfifo",
            "/usr/bin/mknod",
            "/usr/bin/mktemp",
            "/usr/bin/mv",
            "/usr/bin/nice",
            "/usr/bin/nl",
            "/usr/bin/nohup",
            "/usr/bin/nproc",
            "/usr/bin/numfmt",
            "/usr/bin/od",
            "/usr/bin/paste",
            "/usr/bin/pathchk",
            "/usr/bin/pinky",
            "/usr/bin/pr",
            "/usr/bin/printenv",
            "/usr/bin/printf",
            "/usr/bin/ptx",
            "/usr/bin/pwd",
            "/usr/bin/readlink",
            "/usr/bin/realpath",
            "/usr/bin/rm",
            "/usr/bin/rmdir",
            "/usr/bin/runcon",
            "/usr/bin/seq",
            "/usr/bin/sha1sum",
            "/usr/bin/sha224sum",
            "/usr/bin/sha256sum",
            "/usr/bin/sha384sum",
            "/usr/bin/sha512sum",
            "/usr/bin/shred",
            "/usr/bin/shuf",
            "/usr/bin/sleep",
            "/usr/bin/sort",
            "/usr/bin/split",
            "/usr/bin/stat",
            "/usr/bin/stdbuf",
            "/usr/bin/stty",
            "/usr/bin/sum",
            "/usr/bin/sync",
            "/usr/bin/tac",
            "/usr/bin/tail",
            "/usr/bin/tee",
            "/usr/bin/test",
            "/usr/bin/timeout",
            "/usr/bin/touch",
            "/usr/bin/tr",
            "/usr/bin/true",
            "/usr/bin/truncate",
            "/usr/bin/tsort",
            "/usr/bin/tty",
            "/usr/bin/uname",
            "/usr/bin/unexpand",
            "/usr/bin/uniq",
            "/usr/bin/unlink",
            "/usr/bin/uptime",
            "/usr/bin/users",
            "/usr/bin/vdir",
            "/usr/bin/wc",
            "/usr/bin/who",
            "/usr/bin/whoami",
            "/usr/bin/yes",
            "/usr/sbin/chroot",
            // "/usr/bin/chroot",
            // "/usr/bin/coreutils",
        ]);
    }

    if options.diffutils {
        paths.extend([
            "/usr/bin/cmp",
            "/usr/bin/diff",
            "/usr/bin/diff3",
            "/usr/bin/sdiff",
        ]);
    }

    if options.findutils {
        paths.extend([
            "/usr/bin/find",
            "/usr/bin/xargs",
            // "/usr/bin/locate",
            // "/usr/bin/updatedb
        ]);
    }

    if options.file {
        paths.extend([
            "/etc/magic",
            "/usr/bin/file",
            "/usr/lib/file/magic.mgc",
            "/usr/share/file/magic.mgc",
            "/usr/share/misc/magic.mgc",
        ]);
    }

    if options.flex {
        paths.extend(["/usr/bin/flex", "/usr/bin/flex++", "/usr/bin/lex"]);
    }

    if options.gawk {
        paths.extend(["/usr/bin/awk", "/usr/bin/gawk"]);
    }

    if options.gcc {
        paths.extend([
            "/usr/bin/c++",
            "/usr/bin/cc",
            "/usr/bin/cpp",
            "/usr/bin/g++",
            "/usr/bin/gcc",
            "/usr/bin/gcc-ar",
            "/usr/bin/gcc-nm",
            "/usr/bin/gcc-ranlib",
            "/usr/bin/gcov",
            "/usr/bin/gcov-dump",
            "/usr/bin/gcov-tool",
            "/usr/bin/lto-dump",
        ])
    }

    if options.gcc_12 {
        paths.extend(["/usr/lib/gcc/aarch64-linux-gnu/12"]);
    }

    if options.glibc {
        paths.extend([
            "/sbin/ldconfig",
            "/usr/bin/gencat",
            "/usr/bin/getconf",
            "/usr/bin/getent",
            "/usr/bin/iconv",
            "/usr/bin/ld.so",
            "/usr/bin/ldd",
            "/usr/bin/locale",
            "/usr/bin/localedef",
            "/usr/bin/mtrace",
            "/usr/bin/pldd",
            "/usr/bin/sotruss",
            "/usr/bin/sprof",
            "/usr/bin/tzselect",
            "/usr/bin/zdump",
            // "/usr/bin/makedb",
            // "/usr/bin/pcprofiledump",
            // "/usr/bin/xtrace",
        ]);
    }

    if options.grep {
        paths.extend(["/usr/bin/egrep", "/usr/bin/fgrep", "/usr/bin/grep"]);
    }

    if options.gzip {
        paths.extend([
            "/usr/bin/gunzip",
            "/usr/bin/gzexe",
            "/usr/bin/gzip",
            "/usr/bin/uncompress",
            "/usr/bin/zcat",
            "/usr/bin/zcmp",
            "/usr/bin/zdiff",
            "/usr/bin/zegrep",
            "/usr/bin/zfgrep",
            "/usr/bin/zforce",
            "/usr/bin/zgrep",
            "/usr/bin/zless",
            "/usr/bin/zmore",
            "/usr/bin/znew",
        ]);
    }

    if options.help2man {
        paths.extend(["/usr/bin/help2man"]);
    }

    if options.includes {
        paths.extend(["/usr/include"]);
    }

    if options.lib {
        paths.extend(["/usr/lib/aarch64-linux-gnu"]);
    }

    if options.m4 {
        paths.extend(["/usr/bin/m4"]);
    }

    if options.make {
        paths.extend(["/usr/bin/make"]);
    }

    if options.patchelf {
        paths.extend(["/usr/bin/patchelf"]);
    }

    if options.perl {
        paths.extend([
            "/etc/perl",
            "/usr/bin/corelist",
            "/usr/bin/cpan",
            "/usr/bin/enc2xs",
            "/usr/bin/encguess",
            "/usr/bin/h2ph",
            "/usr/bin/h2xs",
            "/usr/bin/instmodsh",
            "/usr/bin/json_pp",
            "/usr/bin/libnetcfg",
            "/usr/bin/perl",
            "/usr/bin/perl",
            "/usr/bin/perl5.36.0",
            "/usr/bin/perlbug",
            "/usr/bin/perldoc",
            "/usr/bin/perlivp",
            "/usr/bin/perlthanks",
            "/usr/bin/piconv",
            "/usr/bin/pl2pm",
            "/usr/bin/pod2html",
            "/usr/bin/pod2man",
            "/usr/bin/pod2text",
            "/usr/bin/pod2usage",
            "/usr/bin/podchecker",
            "/usr/bin/prove",
            "/usr/bin/ptar",
            "/usr/bin/ptardiff",
            "/usr/bin/ptargrep",
            "/usr/bin/shasum",
            "/usr/bin/splain",
            "/usr/bin/streamzip",
            "/usr/bin/xsubpp",
            "/usr/bin/zipdetails",
            "/usr/lib/aarch64-linux-gnu/perl-base",
            "/usr/lib/aarch64-linux-gnu/perl/5.36",
            "/usr/share/perl/5.36",
            "/usr/share/perl5",
        ]);
    }

    if options.python {
        paths.extend([
            "/usr/bin/pydoc3",
            "/usr/bin/pydoc3.11",
            "/usr/bin/python3",
            "/usr/bin/python3.11",
            "/usr/lib/python3.11",
        ]);
    }

    if options.sed {
        paths.extend(["/usr/bin/sed"]);
    }

    if options.tar {
        paths.extend(["/usr/bin/tar"]);
    }

    if options.texinfo {
        paths.extend([
            "/usr/bin/pdftexi2dvi",
            "/usr/bin/pod2texi",
            "/usr/bin/texi2any",
            "/usr/bin/texi2dvi",
            "/usr/bin/texi2pdf",
            "/usr/bin/texindex",
            "/usr/share/texinfo",
            // "/usr/bin/install-info",
            // "/usr/bin/makeinfo",
        ]);
    }

    if options.wget {
        paths.extend(["/usr/bin/wget"]);
    }

    let mut paths = paths
        .iter()
        .map(|p| PackageSandboxPath {
            source: p.to_string(),
            target: p.to_string(),
        })
        .collect::<Vec<PackageSandboxPath>>();

    if options.bash {
        paths.push(PackageSandboxPath {
            source: "/bin/bash".to_string(),
            target: "/bin/sh".to_string(),
        });
    }

    if options.texinfo {
        paths.push(PackageSandboxPath {
            source: "/usr/bin/texi2any".to_string(),
            target: "/usr/bin/makeinfo".to_string(),
        });
    }

    paths
}
