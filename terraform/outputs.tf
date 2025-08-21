output "ssh_private_key_ssm_name" {
  description = "SSM parameter name storing the SSH private key"
  value       = module.ssh_private_key_param.ssm_parameter_name
}

