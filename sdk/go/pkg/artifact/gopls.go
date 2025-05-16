package artifact

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Gopls(context *config.ConfigContext) (*string, error) {
	return context.FetchArtifact("gopls:0.29.0")
}
