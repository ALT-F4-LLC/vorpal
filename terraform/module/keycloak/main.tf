resource "keycloak_realm" "this" {
  realm = local.realm_name
}

resource "keycloak_openid_client" "this" {
  for_each = local.client

  access_type                               = each.value.access_type
  client_id                                 = each.key
  oauth2_device_authorization_grant_enabled = try(each.value.oauth2_device_authorization_grant_enabled, false)
  realm_id                                  = keycloak_realm.this.id
  standard_token_exchange_enabled           = try(each.value.standard_token_exchange_enabled, false)
}

resource "keycloak_role" "this" {
  for_each = tomap({ for r in local.client_roles : "${r.client}-${r.role}" => r })

  client_id = keycloak_openid_client.this[each.value.client].id
  name      = each.value.role
  realm_id  = keycloak_realm.this.id
}

resource "keycloak_openid_client_scope" "this" {
  for_each = local.client_scope

  name     = each.key
  realm_id = keycloak_realm.this.id
}

resource "keycloak_openid_audience_protocol_mapper" "this" {
  for_each = local.client_scope

  add_to_access_token      = true
  add_to_id_token          = false
  client_scope_id          = keycloak_openid_client_scope.this[each.key].id
  included_client_audience = keycloak_openid_client.this[each.value.included_client_audience].client_id
  name                     = "audience-${each.key}"
  realm_id                 = keycloak_realm.this.id
}


resource "keycloak_openid_user_client_role_protocol_mapper" "this" {
  for_each = local.client_scope

  add_to_access_token         = true
  add_to_id_token             = false
  add_to_userinfo             = false
  claim_name                  = "resource_access.$${client_id}.roles"
  client_id_for_role_mappings = keycloak_openid_client.this[each.value.client_id_for_role_mappings].client_id
  client_scope_id             = keycloak_openid_client_scope.this[each.key].id
  multivalued                 = true
  name                        = "roles-${each.key}"
  realm_id                    = keycloak_realm.this.id
}

resource "keycloak_openid_client_optional_scopes" "this" {
  for_each = { for k, v in local.client : "${k}" => v if contains(keys(v), "optional_scopes") }

  client_id       = keycloak_openid_client.this[each.key].id
  optional_scopes = [for scope in each.value.optional_scopes : keycloak_openid_client_scope.this[scope].name]
  realm_id        = keycloak_realm.this.id
}

resource "keycloak_user" "user" {
  for_each = { for user_name, user in local.users : user_name => user }

  email      = each.value.email
  enabled    = true
  first_name = each.value.first_name
  last_name  = each.value.last_name
  realm_id   = keycloak_realm.this.id
  username   = each.key

  initial_password {
    temporary = false
    value     = each.value.password
  }
}
