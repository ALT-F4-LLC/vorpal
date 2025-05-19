package artifact

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Nginx(context *config.ConfigContext) (*string, error) {
	return context.FetchArtifact("nginx:1.27.5")
}
