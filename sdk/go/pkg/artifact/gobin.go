package artifact

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func GoBin(context *config.ConfigContext) (*string, error) {
	return context.FetchArtifact("go:1.24.2")
}
