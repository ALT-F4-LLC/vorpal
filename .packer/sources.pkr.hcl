source "vmware-iso" "debian" {
  boot_command         = var.boot_command
  boot_wait            = "10s"
  cdrom_adapter_type   = var.cdrom_adapter_type
  cpus                 = 2
  disk_adapter_type    = var.disk_adapter_type
  guest_os_type        = var.guest_os_type
  headless             = var.headless
  iso_checksum         = var.iso_checksum
  iso_url              = var.iso_url
  memory               = var.memory
  network_adapter_type = var.network_adapter_type
  output_directory     = ".vagrant/builds/${var.name}"
  shutdown_command     = "echo 'vagrant' | sudo -S shutdown -P now"
  ssh_password         = "vagrant"
  ssh_timeout          = "10000s"
  ssh_username         = "vagrant"
  tools_upload_flavor  = var.tools_upload_flavor
  tools_upload_path    = var.tools_upload_path
  version              = var.hardware_version
  vmx_data             = var.vmx_data

  # Use http_content template to dynamic configure preseed
  # https://www.hashicorp.com/blog/using-template-files-with-hashicorp-packer
  http_content = {
    "/preseed.cfg" = templatefile("${abspath(path.root)}/data/debian/preseed.pkrtpl.hcl", {
      password = var.password
      username = var.username
    })
  }
}
