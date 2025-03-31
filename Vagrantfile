Vagrant.configure("2") do |config|
  config.ssh.password = "vagrant"

  config.vm.box = "altf4llc/debian-bookworm"

  # Speed is important here as a lot of compiling is done in the vm
  # Be sure to set a high enough value for your system
  config.vm.provider :vmware_desktop do |vmware|
    vmware.vmx["ethernet0.pcislotnumber"] = "160"
    vmware.vmx["memsize"] = "16384"
    vmware.vmx["numvcpus"] = "8"
  end

  config.vm.provision "shell", keep_color: true, privileged: false, inline: <<-SHELL
    echo 'function sync_vorpal {
      mkdir -p $HOME/vorpal

      rsync -aPW \
        --delete \
        --exclude=".env" \
        --exclude=".git" \
        --exclude=".packer" \
        --exclude=".vagrant" \
        --exclude="dist" \
        --exclude="packer_debian_vmware_arm64.box" \
        --exclude="target" \
        /vagrant/. $HOME/vorpal/.
    }' >> ~/.bashrc

    echo 'function setup_vorpal {
      sync_vorpal

      pushd $HOME/vorpal

      ./script/dev.sh make dist
      ./script/install.sh

      popd
    }' >> ~/.bashrc

  echo "PATH=\"${HOME}/vorpal/.env/bin:${HOME}/.cargo/bin:\${PATH}\"" >> ~/.bashrc
  SHELL
end
