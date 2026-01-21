package artifact

import (
	"fmt"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func BuildVorpalJob(context *config.ConfigContext) (*string, error) {
	vorpal, err := BuildVorpal(context)
	if err != nil {
		return nil, fmt.Errorf("failed to build vorpal: %w", err)
	}

	script := fmt.Sprintf("\n%s/bin/vorpal --version", artifact.GetEnvKey(vorpal))

	return artifact.NewTask("vorpal-job", script, SYSTEMS).
		WithArtifacts([]*string{vorpal}).
		Build(context)
}
