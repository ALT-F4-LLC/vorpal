data "aws_availability_zones" "available" {}

# Ubuntu 24.04 (Noble) AMIs via aws_ami lookup requiring gp3 volumes (Canonical owner)
data "aws_ami" "ubuntu_2404_arm64" {
  most_recent = true
  owners      = ["099720109477"] # Canonical

  filter {
    name   = "name"
    values = ["ubuntu/images/hvm-ssd-gp3/ubuntu-noble-24.04-arm64-server-*"]
  }

  filter {
    name   = "architecture"
    values = ["arm64"]
  }

  filter {
    name   = "virtualization-type"
    values = ["hvm"]
  }

  filter {
    name   = "root-device-type"
    values = ["ebs"]
  }

  filter {
    name   = "block-device-mapping.volume-type"
    values = ["gp3"]
  }

  filter {
    name   = "state"
    values = ["available"]
  }
}

data "aws_ami" "ubuntu_2404_x86_64" {
  most_recent = true
  owners      = ["099720109477"] # Canonical

  filter {
    name   = "name"
    values = ["ubuntu/images/hvm-ssd-gp3/ubuntu-noble-24.04-amd64-server-*"]
  }

  filter {
    name   = "architecture"
    values = ["x86_64"]
  }

  filter {
    name   = "virtualization-type"
    values = ["hvm"]
  }

  filter {
    name   = "root-device-type"
    values = ["ebs"]
  }

  filter {
    name   = "block-device-mapping.volume-type"
    values = ["gp3"]
  }

  filter {
    name   = "state"
    values = ["available"]
  }
}

data "aws_ssm_parameter" "mac_arm64" {
  name = "/aws/service/ec2-macos/sequoia/arm64_mac/latest/image_id"
}

data "aws_ssm_parameter" "mac_x86_64" {
  name = "/aws/service/ec2-macos/sequoia/x86_64_mac/latest/image_id"
}

locals {
  az = data.aws_availability_zones.available.names[0]
}

module "vpc" {
  source  = "terraform-aws-modules/vpc/aws"
  version = "6.0.1"

  azs                  = [local.az]
  cidr                 = "10.42.0.0/16"
  enable_dns_hostnames = true
  enable_dns_support   = true
  enable_nat_gateway   = false
  name                 = "vorpal-dev"
  public_subnets       = ["10.42.0.0/24"]
  single_nat_gateway   = false

  public_subnet_tags = {
    Name = "vorpal-dev-public"
  }
}

module "sg_ssh" {
  source  = "terraform-aws-modules/security-group/aws"
  version = "5.3.0"

  description         = "Allow SSH"
  egress_rules        = ["all-all"]
  ingress_cidr_blocks = [var.ssh_ingress_cidr]
  ingress_rules       = ["all-all"]
  ingress_with_self   = [{ rule = "all-all" }]
  name                = "vorpal-dev-ssh"
  vpc_id              = module.vpc.vpc_id
}

module "key_pair" {
  source  = "terraform-aws-modules/key-pair/aws"
  version = "2.1.0"

  create_private_key = true
  key_name           = "vorpal-dev"
}

module "ssh_private_key_param" {
  source  = "terraform-aws-modules/ssm-parameter/aws"
  version = "~> 1.0"

  description = "SSH private key for Vorpal dev instances"
  name        = "/vorpal-dev/private-key"
  type        = "SecureString"
  value       = module.key_pair.private_key_openssh
}

module "instance_registry" {
  source  = "terraform-aws-modules/ec2-instance/aws"
  version = "6.1.1"

  ami                         = data.aws_ami.ubuntu_2404_arm64.id
  associate_public_ip_address = true
  create_security_group       = false
  instance_type               = "t4g.large"
  key_name                    = module.key_pair.key_pair_name
  name                        = "vorpal-dev-registry"
  subnet_id                   = module.vpc.public_subnets[0]
  vpc_security_group_ids      = [module.sg_ssh.security_group_id]

  root_block_device = {
    size = 100
  }
}

module "instance_worker_aarch64_linux" {
  source  = "terraform-aws-modules/ec2-instance/aws"
  version = "6.1.1"

  ami                         = data.aws_ami.ubuntu_2404_arm64.id
  associate_public_ip_address = true
  create_security_group       = false
  instance_type               = "t4g.large"
  key_name                    = module.key_pair.key_pair_name
  name                        = "vorpal-dev-worker-aarch64-linux"
  subnet_id                   = module.vpc.public_subnets[0]
  vpc_security_group_ids      = [module.sg_ssh.security_group_id]

  root_block_device = {
    size = 100
  }
}

module "instance_worker_x8664_linux" {
  source  = "terraform-aws-modules/ec2-instance/aws"
  version = "6.1.1"

  ami                         = data.aws_ami.ubuntu_2404_x86_64.id
  associate_public_ip_address = true
  create_security_group       = false
  instance_type               = "t3a.large"
  key_name                    = module.key_pair.key_pair_name
  name                        = "vorpal-dev-worker-x8664-linux"
  subnet_id                   = module.vpc.public_subnets[0]
  vpc_security_group_ids      = [module.sg_ssh.security_group_id]

  root_block_device = {
    size = 100
  }
}

resource "aws_ec2_host" "worker_aarch64_darwin" {
  count = var.create_mac_instances ? 1 : 0

  availability_zone = local.az
  instance_type     = "mac2.metal"
}

module "instance_worker_aarch64_darwin" {
  source  = "terraform-aws-modules/ec2-instance/aws"
  version = "6.1.1"

  count = var.create_mac_instances ? 1 : 0

  ami                         = data.aws_ssm_parameter.mac_arm64.value
  associate_public_ip_address = true
  availability_zone           = local.az
  create_security_group       = false
  host_id                     = aws_ec2_host.worker_aarch64_darwin[0].id
  instance_type               = "mac2.metal"
  key_name                    = module.key_pair.key_pair_name
  name                        = "vorpal-dev-worker-aarch64-darwin"
  subnet_id                   = module.vpc.public_subnets[0]
  tenancy                     = "host"
  vpc_security_group_ids      = [module.sg_ssh.security_group_id]

  root_block_device = {
    size = 100
  }
}

resource "aws_ec2_host" "worker_x8664_darwin" {
  count = var.create_mac_instances ? 1 : 0

  availability_zone = local.az
  instance_type     = "mac1.metal"
}

module "instance_worker_x8664_darwin" {
  source  = "terraform-aws-modules/ec2-instance/aws"
  version = "6.1.1"

  count = var.create_mac_instances ? 1 : 0

  ami                         = data.aws_ssm_parameter.mac_x86_64.value
  associate_public_ip_address = true
  availability_zone           = local.az
  create_security_group       = false
  host_id                     = aws_ec2_host.worker_x8664_darwin[0].id
  instance_type               = "mac1.metal"
  key_name                    = module.key_pair.key_pair_name
  name                        = "vorpal-dev-worker-x8664-darwin"
  subnet_id                   = module.vpc.public_subnets[0]
  tenancy                     = "host"
  vpc_security_group_ids      = [module.sg_ssh.security_group_id]

  root_block_device = {
    size = 100
  }
}
