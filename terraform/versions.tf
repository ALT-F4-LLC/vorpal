terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "6.27.0"
    }

    keycloak = {
      source  = "keycloak/keycloak"
      version = "5.4.0"
    }
  }
}
