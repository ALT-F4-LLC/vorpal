use indoc::formatdoc;

pub fn script(
    bison_version: &str,
    gettext_version: &str,
    perl_version: &str,
    python_version: &str,
    texinfo_version: &str,
    util_linux_version: &str,
) -> String {
    formatdoc! {"
        ## Setup paths

        export VORPAL_SOURCE=\"$(pwd)/source\"

        ## Setup environment

        export MAKEFLAGS=\"-j$(nproc)\"

        ## Setup system directories

        mkdir -pv /{{boot,home,mnt,opt,srv}}
        mkdir -pv /etc/{{opt,sysconfig}}
        mkdir -pv /lib/firmware
        mkdir -pv /media/{{floppy,cdrom}}
        mkdir -pv /usr/{{,local/}}{{include,src}}
        mkdir -pv /usr/lib/locale
        mkdir -pv /usr/local/{{bin,lib,sbin}}
        mkdir -pv /usr/{{,local/}}share/{{color,dict,doc,info,locale,man}}
        mkdir -pv /usr/{{,local/}}share/{{misc,terminfo,zoneinfo}}
        mkdir -pv /usr/{{,local/}}share/man/man{{1..8}}
        mkdir -pv /var/{{cache,local,log,mail,opt,spool}}
        mkdir -pv /var/lib/{{color,misc,locate}}

        ## Setup root

        install -dv -m 0750 /root

        ## Setup system files

        cat > /etc/hosts << \"EOF\"
        127.0.0.1  localhost
        ::1        localhost
        EOF

        cat > /etc/passwd << \"EOF\"
        root:x:0:0:root:/root:/bin/bash
        bin:x:1:1:bin:/dev/null:/usr/bin/false
        daemon:x:6:6:Daemon User:/dev/null:/usr/bin/false
        messagebus:x:18:18:D-Bus Message Daemon User:/run/dbus:/usr/bin/false
        uuidd:x:80:80:UUID Generation Daemon User:/dev/null:/usr/bin/false
        nobody:x:65534:65534:Unprivileged User:/dev/null:/usr/bin/false
        EOF

        cat > /etc/group << \"EOF\"
        root:x:0:
        bin:x:1:daemon
        sys:x:2:
        kmem:x:3:
        tape:x:4:
        tty:x:5:
        daemon:x:6:
        floppy:x:7:
        disk:x:8:
        lp:x:9:
        dialout:x:10:
        audio:x:11:
        video:x:12:
        utmp:x:13:
        cdrom:x:15:
        adm:x:16:
        messagebus:x:18:
        input:x:24:
        mail:x:34:
        kvm:x:61:
        uuidd:x:80:
        wheel:x:97:
        users:x:999:
        nogroup:x:65534:
        EOF

        ## Setup locale

        localedef -i C -f UTF-8 C.UTF-8

        ## Setup logs

        touch /var/log/{{btmp,lastlog,faillog,wtmp}}

        ## Setup resolv.conf

        echo 'nameserver 1.1.1.1' > /etc/resolv.conf

        ## Build gettext

        mkdir -pv $VORPAL_SOURCE/gettext/gettext-{gettext_version}/build
        pushd $VORPAL_SOURCE/gettext/gettext-{gettext_version}/build

        ../configure --disable-shared

        make

        cp -pv gettext-tools/src/{{msgfmt,msgmerge,xgettext}} /usr/bin

        popd

        rm -rf $VORPAL_SOURCE/gettext

        ## Build bison

        mkdir -pv $VORPAL_SOURCE/bison/bison-{bison_version}/build
        pushd $VORPAL_SOURCE/bison/bison-{bison_version}/build

        ../configure \
            --prefix=\"/usr\" \
            --docdir=\"/usr/share/doc/bison-3.8.2\"

        make
        make install

        popd

        rm -rf $VORPAL_SOURCE/bison

        ## Build perl

        pushd $VORPAL_SOURCE/perl/perl-{perl_version}

        sh Configure \
            -des \
            -D prefix=\"/usr\" \
            -D vendorprefix=\"/usr\" \
            -D useshrplib \
            -D privlib=\"/usr/lib/perl5/5.40/core_perl\" \
            -D archlib=\"/usr/lib/perl5/5.40/core_perl\" \
            -D sitelib=\"/usr/lib/perl5/5.40/site_perl\" \
            -D sitearch=\"/usr/lib/perl5/5.40/site_perl\" \
            -D vendorlib=\"/usr/lib/perl5/5.40/vendor_perl\" \
            -D vendorarch=\"/usr/lib/perl5/5.40/vendor_perl\"

        make
        make install

        popd

        rm -rf $VORPAL_SOURCE/perl

        ## Build Python

        mkdir -pv $VORPAL_SOURCE/python/Python-{python_version}/build
        pushd $VORPAL_SOURCE/python/Python-{python_version}/build

        ../configure \
            --prefix=\"/usr\" \
            --enable-shared \
            --without-ensurepip

        make
        make install

        popd

        rm -rf $VORPAL_SOURCE/python

        ## Build texinfo

        mkdir -pv $VORPAL_SOURCE/texinfo/texinfo-{texinfo_version}/build
        pushd $VORPAL_SOURCE/texinfo/texinfo-{texinfo_version}/build

        ../configure --prefix=\"/usr\"

        make
        make install

        popd

        rm -rf $VORPAL_SOURCE/texinfo

        ## Build util-linux

        mkdir -pv $VORPAL_SOURCE/util-linux/util-linux-{util_linux_version}/build
        pushd $VORPAL_SOURCE/util-linux/util-linux-{util_linux_version}/build

        mkdir -pv /var/lib/hwclock

        # note: \"--disable-makeinstall-chown\" for sandbox limitations

        ../configure \
            --libdir=\"/usr/lib\" \
            --runstatedir=\"/run\" \
            --disable-chfn-chsh \
            --disable-login \
            --disable-nologin \
            --disable-su \
            --disable-setpriv \
            --disable-runuser \
            --disable-pylibmount \
            --disable-static \
            --disable-liblastlog2 \
            --disable-makeinstall-chown \
            --without-python \
            ADJTIME_PATH=\"/var/lib/hwclock/adjtime\" \
            --docdir=\"/usr/share/doc/util-linux-2.40.2\"

        make
        make install

        popd

        rm -rf $VORPAL_SOURCE/util-linux

        ## Cleanup

        rm -rf /usr/share/{{info,man,doc}}/*

        find /usr/{{lib,libexec}} -name \\*.la -delete",
    }
}
