Vagrant.configure("2") do |config|
  config.ssh.password = "vagrant"

  config.vm.box = "altf4llc/debian-bookworm"

  config.vm.provider :vmware_desktop do |vmware|
    vmware.vmx["memsize"] = "8192"
    vmware.vmx["numvcpus"] = "8"
  end

  config.vm.provision "shell", keep_color: true, privileged: false, inline: <<-SHELL
    echo 'function setup_vorpal {
      rm -rf ${HOME}/vorpal

      mkdir -p ${HOME}/vorpal

      rsync -aPW \
        --exclude=".git" \
        --exclude=".vagrant" \
        --exclude=".vorpal/env" \
        --exclude="target" \
        /vagrant/. ./vorpal/.
    }' >> ~/.bashrc

    echo 'function setup_dev {
      setup_vorpal

      cd ${HOME}/vorpal

      ./script/setup/debian.sh "dev"
      ./script/setup/dev.sh

      PATH=$PWD/.vorpal/env/bin:$HOME/.cargo/bin:$PATH

      make dist
    }' >> ~/.bashrc

    echo 'function setup_sandbox {
      setup_vorpal

      cd ${HOME}/vorpal

      ./script/setup/debian.sh "sandbox"
      ./script/setup/sandbox.sh
    }' >> ~/.bashrc
  SHELL
end
