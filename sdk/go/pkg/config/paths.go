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

// getKeyCredentialsPathDefault is the default implementation
func getKeyCredentialsPathDefault() string {
	return filepath.Join(GetRootKeyDirPath(), "credentials.json")
}

// getKeyCredentialsPathFunc is a variable that can be replaced for testing
var getKeyCredentialsPathFunc = getKeyCredentialsPathDefault

// GetKeyCredentialsPath returns the credentials file path
func GetKeyCredentialsPath() string {
	return getKeyCredentialsPathFunc()
}
