package artifact

import (
	"fmt"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func BuildVorpalContainerImage(context *config.ConfigContext) (*string, error) {
	rootfs, err := artifact.LinuxVorpalSlim(context)
	if err != nil {
		return nil, fmt.Errorf("failed to build vorpal: %w", err)
	}

	return artifact.NewOciImage("vorpal-container-image", *rootfs).
		Build(context)
}
