package artifact

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Crane(context *config.ConfigContext) (*string, error) {
	return context.FetchArtifactAlias("crane:0.20.7")
}
