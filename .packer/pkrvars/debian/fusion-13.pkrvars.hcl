boot_command         = ["<wait><up>e<wait><down><down><down><right><right><right><right><right><right><right><right><right><right><right><right><right><right><right><right><right><right><right><right><right><right><right><right><right><right><right><right><right><right><right><right><right><right><wait>install <wait> preseed/url=http://{{ .HTTPIP }}:{{ .HTTPPort }}/preseed.cfg <wait>debian-installer=en_US.UTF-8 <wait>auto <wait>locale=en_US.UTF-8 <wait>kbd-chooser/method=us <wait>keyboard-configuration/xkb-keymap=us <wait>netcfg/get_hostname={{ .Name }} <wait>netcfg/get_domain={{ .Name }} <wait>fb=false <wait>debconf/frontend=noninteractive <wait>console-setup/ask_detect=false <wait>console-keymaps-at/keymap=us <wait>grub-installer/bootdev=/dev/sda <wait><f10><wait>"]
cdrom_adapter_type   = "sata"
disk_adapter_type    = "sata"
guest_os_type        = "arm-debian-64"
hardware_version     = 21
iso_checksum         = "93646d88c7ce54f8a2a846f938cdcf25a9123c36c3788208b27f5fbad7bbd855"
iso_url              = "https://cdimage.debian.org/cdimage/archive/12.6.0/arm64/iso-dvd/debian-12.6.0-arm64-DVD-1.iso"
name              = "debian_aarch64"
network_adapter_type = "e1000e"

vmx_data = {
  "cpuid.coresPerSocket"    = "2"
  "ethernet0.pciSlotNumber" = "160"
  "svga.autodetect"         = true
  "usb_xhci.present"        = true
}
