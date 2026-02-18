terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "6.33.0"
    }

    keycloak = {
      source  = "keycloak/keycloak"
      version = "5.6.0"
    }
  }
}
