package artifact

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Gh(context *config.ConfigContext) (*string, error) {
	return context.FetchArtifact("gh:2.69.0")
}
