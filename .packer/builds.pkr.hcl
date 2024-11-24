build {
  sources = ["source.vmware-iso.debian"]

  provisioner "shell" {
    inline = [
      "sudo apt-get update",
      "sudo apt-get upgrade --yes",
      "sudo apt-get install --yes open-vm-tools",
    ]
  }

  post-processor "vagrant" {}
}
