terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "6.47.0"
    }

    keycloak = {
      source  = "keycloak/keycloak"
      version = "5.7.0"
    }
  }
}
