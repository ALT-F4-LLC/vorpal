package artifact

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func NodeJS(context *config.ConfigContext) (*string, error) {
	return context.FetchArtifactAlias("nodejs:22.14.0")
}
