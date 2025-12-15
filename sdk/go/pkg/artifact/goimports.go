package artifact

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Goimports(context *config.ConfigContext) (*string, error) {
	return context.FetchArtifactAlias("goimports:0.29.0")
}
