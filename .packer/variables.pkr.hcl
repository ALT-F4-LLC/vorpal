variable "boot_command" {
  default = ["<esc><wait>", "<esc><wait>", "<enter><wait>", "/install/vmlinuz<wait>", " initrd=/install/initrd.gz", " auto-install/enable=true", " debconf/priority=critical", " preseed/url=http://{{ .HTTPIP }}:{{ .HTTPPort }}/preseed.cfg<wait>", " -- <wait>", "<enter><wait>"]
  type    = list(string)
}

variable "cdrom_adapter_type" {
  default = "sata"
  type    = string
}

variable "disk_size" {
  default = 65536
  type    = number
}

variable "disk_adapter_type" {
  default = "lsilogic"
  type    = string
}

variable "guest_os_type" {
  default = null
  type    = string
}

variable "hardware_version" {
  default = 21
  type    = number
}

variable "headless" {
  default = true
  type    = bool
}

variable "iso_checksum" {
  default = null
  type    = string
}

variable "iso_url" {
  default = null
  type    = string
}

variable "memory" {
  default = 2048
  type    = number
}

variable "name" {
  default = "debian"
  type    = string
}

variable "network_adapter_type" {
  default = null
  type    = string
}

variable "password" {
  default = "vagrant"
  type    = string
}

variable "tools_upload_flavor" {
  default = null
  type    = string
}

variable "tools_upload_path" {
  default = null
  type    = string
}

variable "username" {
  default = "vagrant"
  type    = string
}

variable "vmx_data" {
  default = {
    "cpuid.coresPerSocket" = "2"
  }
  type = map(string)
}
