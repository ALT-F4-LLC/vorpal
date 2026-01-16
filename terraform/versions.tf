terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "6.28.0"
    }

    keycloak = {
      source  = "keycloak/keycloak"
      version = "5.6.0"
    }
  }
}
