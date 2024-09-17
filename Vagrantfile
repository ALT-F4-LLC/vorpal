Vagrant.configure("2") do |config|
  config.ssh.password = "vagrant"

  config.vm.box = "altf4llc/debian-bookworm"

  # Speed is important here as a lot of compiling is done in the vm
  # Be sure to set a high enough value for your system
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
        --exclude="packer_debian_vmware_arm64.box" \
        --exclude="target" \
        /vagrant/. ./vorpal/.

      popd
    }' >> ~/.bashrc

    echo 'function setup_dev {
      setup_vorpal

      pushd "${HOME}/vorpal"

      ./script/debian.sh
      ./script/dev.sh make dist
      ./script/install.sh

      popd
    }' >> ~/.bashrc

    echo "PATH=\"${HOME}/vorpal/.env/bin:\${PATH}\"" >> ~/.bashrc
  SHELL
end
