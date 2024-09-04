build {
  sources = ["source.vmware-iso.debian"]

  provisioner "shell" {
    inline = [
      "sudo apt-get update",
      "sudo apt-get upgrade --yes",
      "sudo apt-get install --yes curl open-vm-tools",
    ]
  }

  post-processor "vagrant" {}
}
