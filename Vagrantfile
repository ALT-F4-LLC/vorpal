Vagrant.configure("2") do |config|
  config.ssh.password = "vagrant"

  config.vm.box = "altf4llc/debian-bookworm"

  config.vm.provider :vmware_desktop do |vmware|
    vmware.vmx["memsize"] = "8192"
    vmware.vmx["numvcpus"] = "4"
  end

  config.vm.provision "shell", keep_color: true, privileged: false, inline: <<-SHELL
    set -euo pipefail

    mkdir -p ./vorpal

    rsync -aPW \
      --exclude='.git' \
      --exclude='.vagrant' \
      --exclude='.vorpal/env' \
      --exclude='target' \
      /vagrant/. ./vorpal/.

    cd ./vorpal

    ./script/setup/debian.sh "dev"
    ./script/setup/dev.sh

    PATH=$PWD/.vorpal/env/bin:$HOME/.cargo/bin:$PATH

    make dist
  SHELL
end
