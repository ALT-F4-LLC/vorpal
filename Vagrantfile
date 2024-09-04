Vagrant.configure("2") do |config|
  config.ssh.password = "vagrant"

  config.vm.box = "altf4llc/debian-bookworm"

  config.vm.provider :vmware_desktop do |vmware|
    vmware.vmx["memsize"] = "8192"
    vmware.vmx["numvcpus"] = "4"
  end

  config.vm.provision "file",
    destination: "$HOME/script",
    source: "./script"

  # config.vm.provision "shell",
  #   keep_color: true,
  #   path: "./script/setup/sandbox.sh",
  #   privileged: false
end
