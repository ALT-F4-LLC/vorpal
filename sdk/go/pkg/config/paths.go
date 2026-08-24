package config

import "path/filepath"

// GetRootDirPath returns the Vorpal root directory
func GetRootDirPath() string {
	return "/var/lib/vorpal"
}

// GetRootKeyDirPath returns the key directory path
func GetRootKeyDirPath() string {
	return filepath.Join(GetRootDirPath(), "key")
}

// GetKeyCredentialsPath returns the credentials file path. Deliberately not
// configurable from any production entry point (env var, global override) —
// tests drive clientAuthHeaderAt, which takes the credentials path as a
// parameter, instead of redirecting this function.
func GetKeyCredentialsPath() string {
	return filepath.Join(GetRootKeyDirPath(), "credentials.json")
}

// GetKeyCaPath returns the path to the CA certificate
func GetKeyCaPath() string {
	return filepath.Join(GetRootKeyDirPath(), "ca.pem")
}
