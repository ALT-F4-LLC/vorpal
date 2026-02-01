package artifact

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Rsync(context *config.ConfigContext) (*string, error) {
	return context.FetchArtifactAlias("rsync:3.4.1")
}
