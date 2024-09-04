Vagrant.configure("2") do |config|
  config.ssh.password = "vagrant"

  config.vm.box = "altf4llc/debian-bookworm"

  config.vm.provider :vmware_desktop do |vmware|
    vmware.vmx["memsize"] = "8192"
    vmware.vmx["numvcpus"] = "2"
  end

  config.vm.provision "file", source: "./sandbox.sh", destination: "$HOME/sandbox.sh"
  config.vm.provision "shell", path: "./sandbox.sh"
end
