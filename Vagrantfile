Vagrant.configure("2") do |config|
  config.ssh.password = "vagrant"

  config.vm.box = "altf4llc/debian-bookworm"

  config.vm.provider :vmware_desktop do |vmware|
    vmware.vmx["memsize"] = "8192"
    vmware.vmx["numvcpus"] = "8"
  end

  config.vm.provision "shell", keep_color: true, privileged: false, inline: <<-SHELL
    echo 'function setup_vorpal {
      pushd "${HOME}"

      mkdir -p ./vorpal

      rsync -aPW \
        --exclude=".env" \
        --exclude=".git" \
        --exclude=".packer" \
        --exclude=".vagrant" \
        --exclude="dist" \
        --exclude="target" \
        /vagrant/. ./vorpal/.

      popd
    }' >> ~/.bashrc

    echo 'function setup_dev {
      setup_vorpal

      pushd "${HOME}/vorpal"

      ./script/debian.sh "dev"
      ./script/dev.sh

      PATH=$PWD/.env/bin:$HOME/.cargo/bin:$PATH

      make dist

      popd
    }' >> ~/.bashrc

    echo 'function setup_sandbox {
      setup_dev

      pushd "${HOME}/vorpal"

      ./script/debian.sh "sandbox"
      ./script/sandbox.sh

      popd
    }' >> ~/.bashrc
  SHELL
end
