package artifact

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func BuildVorpalUser(context *config.ConfigContext) (*string, error) {
	return artifact.
		NewUserEnvironment("vorpal-user", SYSTEMS).
		WithArtifacts([]*string{}).
		WithEnvironments([]string{"PATH=$HOME/.vorpal/bin"}).
		WithSymlinks(map[string]string{
			"$HOME/Development/repository/github.com/ALT-F4-LLC/vorpal.git/main/target/debug/vorpal": "$HOME/.vorpal/bin/vorpal",
		}).
		Build(context)
}
