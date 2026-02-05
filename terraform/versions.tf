terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "6.31.0"
    }

    keycloak = {
      source  = "keycloak/keycloak"
      version = "5.6.0"
    }
  }
}
