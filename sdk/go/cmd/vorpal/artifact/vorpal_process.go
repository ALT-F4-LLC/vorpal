package artifact

import (
	"fmt"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func BuildVorpalProcess(context *config.ConfigContext) (*string, error) {
	vorpal, err := Vorpal(context)
	if err != nil {
		return nil, fmt.Errorf("failed to build vorpal: %w", err)
	}

	return artifact.NewProcess(
		"vorpal-process",
		fmt.Sprintf("%s/bin/vorpal", artifact.GetEnvKey(*vorpal)),
		SYSTEMS,
	).
		WithArguments([]string{
			"--registry",
			"https://localhost:50051",
			"services",
			"start",
			"--port",
			"50051",
		}).
		WithArtifacts([]*string{vorpal}).
		Build(context)
}
