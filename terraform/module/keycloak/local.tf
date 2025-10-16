locals {
  realm_name = "vorpal"

  client = {
    cli = {
      access_type                               = "PUBLIC"
      oauth2_device_authorization_grant_enabled = true
      optional_scopes = [
        "archive",
        "artifact",
        "worker"
      ]
    }

    archive = {
      access_type                     = "CONFIDENTIAL"
      standard_token_exchange_enabled = true
      roles = [
        "archive:check",
        "archive:push",
        "archive:pull"
      ]
    }

    artifact = {
      access_type                     = "CONFIDENTIAL"
      standard_token_exchange_enabled = true
      roles = [
        "artifact:get",
        "artifact:get-alias",
        "artifact:store"
      ]
    }

    worker = {
      access_type                     = "CONFIDENTIAL"
      standard_token_exchange_enabled = true
      optional_scopes = [
        "archive",
        "artifact",
      ]
      roles = [
        "worker:build-artifact",
      ]
    }
  }

  client_scope = {
    archive = {
      client_id_for_role_mappings = "archive"
      included_client_audience    = "archive"
    }

    artifact = {
      client_id_for_role_mappings = "artifact"
      included_client_audience    = "artifact"
    }

    worker = {
      client_id_for_role_mappings = "worker"
      included_client_audience    = "worker"
    }
  }

  client_roles = flatten([
    for client_name, client in local.client : [
      for role in try(client.roles, []) : {
        client = client_name
        role   = role
      }
    ]
  ])

  users = {
    "admin" = {
      email      = "admin@localhost"
      first_name = "Admin"
      last_name  = "User"
      password   = "password"
    }
  }
}
