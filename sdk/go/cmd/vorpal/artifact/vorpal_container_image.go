package artifact

import (
	"fmt"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func BuildVorpalContainerImage(context *config.ConfigContext) (*string, error) {
	linuxVorpalSlim, err := artifact.LinuxVorpalSlim(context)
	if err != nil {
		return nil, fmt.Errorf("failed to build vorpal: %w", err)
	}

	vorpal, err := Vorpal(context)
	if err != nil {
		return nil, fmt.Errorf("failed to build vorpal: %w", err)
	}

	name := "vorpal-container-image"

	return artifact.NewOciImage(name, *linuxVorpalSlim).
		WithAliases([]string{fmt.Sprintf("%s:latest", name)}).
		WithArtifacts([]*string{vorpal}).
		Build(context)
}
