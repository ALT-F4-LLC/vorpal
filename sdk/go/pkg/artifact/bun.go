package artifact

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Bun(context *config.ConfigContext) (*string, error) {
	return context.FetchArtifactAlias("bun:1.2.0")
}
