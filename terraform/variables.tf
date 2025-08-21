variable "create_mac_instances" {
  description = "Whether to launch macOS instances (requires Dedicated Hosts)"
  type        = bool
  default     = false
}

variable "ssh_ingress_cidr" {
  description = "CIDR allowed to SSH to instances"
  type        = string
  default     = "0.0.0.0/0"
}
